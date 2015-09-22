// https://github.com/Eljay/assimp-rs
extern crate assimp;
extern crate pyramid;
extern crate pyramid_animation;
extern crate cgmath;
extern crate mesh;
extern crate time;

use pyramid::*;
use pyramid::document::*;
use pyramid::pon::*;
use pyramid_animation as pa;
use assimp::*;
use std::path::Path;
use std::rc::Rc;
use time::*;

pub fn init_logging() {
    LogStream::set_verbose_logging(true);
    let mut log_stream = LogStream::stdout();
    log_stream.attach();
}

pub struct Asset3d {
    id: String,
    scene: Scene<'static>,

    // For some reason the .x texcoords are inverted in assimp on line https://github.com/assimp/assimp/blob/f3d418a199cfb7864c826665016e11c65ddd7aa9/code/XFileImporter.cpp#L353
    invert_texcoord_y: bool
}

impl Asset3d {
    pub fn from_file(path: &Path) -> Asset3d {
        println!("Loading asset3d from file {}", path.to_str().unwrap());
        let mut importer = Importer::new();
        importer.triangulate(true);

        Asset3d {
            id: path.to_str().unwrap().to_string(),
            scene: importer.read_file(path.to_str().unwrap()).unwrap(),
            invert_texcoord_y: path.extension().unwrap() == "x"
        }
    }
    pub fn from_string(asset_id: &str, content: &str) -> Asset3d {
        let mut importer = Importer::new();
        importer.triangulate(true);

        Asset3d {
            id: asset_id.to_string(),
            scene: importer.read_string(content).unwrap(),
            invert_texcoord_y: false
        }
    }
    fn get_mesh_key(&self, mesh_id: usize) -> String {
        format!("{}.meshes.{}", self.id, mesh_id)
    }
    fn get_anim_key(&self, anim_name: &str) -> String {
        format!("{}.animations.{}", self.id, anim_name)
    }
    pub fn add_resources_to_document(&self, document: &mut Document) {
        for mesh_id in 0..self.scene.num_meshes() {
            let aimesh = self.scene.mesh(mesh_id as usize).unwrap();
            let mesh = self.mesh_from_assimp_mesh(aimesh);
            document.resources.insert(self.get_mesh_key(mesh_id as usize), Box::new(Rc::new(mesh)));
        }
        for animation_id in 0..self.scene.num_animations() {
            let aianim = self.scene.animation(animation_id as usize).unwrap();
            let key = self.get_anim_key(aianim.name.as_ref());
            let anim = self.track_set_from_assimp_animation(aianim);
            document.resources.insert(key, Box::new(Rc::new(anim)));
        }
    }
    pub fn append_to_document(&self, mut document: &mut Document, parent_id: Option<EntityId>) {
        let parent_id = match parent_id {
            Some(id) => id,
            None => document.append_entity(None, "RootEntity", None).unwrap()
        };
        for animation_id in 0..self.scene.num_animations() {
            let aianim = self.scene.animation(animation_id as usize).unwrap();
            document.set_property(&parent_id, &format!("animation_{}", aianim.name.as_ref()),
                Pon::new_typed_pon("track_set_from_resource", Pon::String(self.get_anim_key(aianim.name.as_ref())))).unwrap();
        }
        self.append_node_to_entity(self.scene.root_node(), &mut document, parent_id);
    }
    fn append_node_to_entity(&self, node: Node, mut document: &mut Document, parent_id: EntityId) {
        let entity_id = document.append_entity(Some(parent_id), "Entity", Some(node.name().to_string())).unwrap();
        document.set_property(&entity_id, "diffuse", Pon::DependencyReference(NamedPropRef::new(EntityPath::Parent, "diffuse"), None)).unwrap();
        document.set_property(&entity_id, "translation", Pon::Vector3(cgmath::Vector3::new(0.0, 0.0, 0.0))).unwrap();
        document.set_property(&entity_id, "rotation", Pon::Vector4(cgmath::Vector4::new(0.0, 0.0, 0.0, 1.0))).unwrap();
        document.set_property(&entity_id, "scale", Pon::Vector3(cgmath::Vector3::new(1.0, 1.0, 1.0))).unwrap();

        let local_transform: cgmath::Matrix4<f32> = mat_into(node.transformation());
        document.set_property(&entity_id, "local_transform", Pon::Matrix4(local_transform)).unwrap();
        let mut transforms = vec![];

        transforms.push(Pon::DependencyReference(NamedPropRef::new(EntityPath::Parent, "transform"), None));
        transforms.push(Pon::DependencyReference(NamedPropRef::new(EntityPath::This, "local_transform"), None));
        transforms.push(Pon::new_typed_pon("translate", Pon::DependencyReference(NamedPropRef::new(EntityPath::This, "translation"), None)));
        transforms.push(Pon::new_typed_pon("rotate_quaternion", Pon::DependencyReference(NamedPropRef::new(EntityPath::This, "rotation"), None)));
        transforms.push(Pon::new_typed_pon("scale", Pon::DependencyReference(NamedPropRef::new(EntityPath::This, "scale"), None)));

        document.set_property(&entity_id, "transform", Pon::new_typed_pon("mul", Pon::Array(transforms))).unwrap();

        document.set_property(&entity_id, "shader", Pon::DependencyReference(NamedPropRef::new(EntityPath::Parent, "shader"), None)).unwrap();
        document.set_property(&entity_id, "uniforms", Pon::DependencyReference(NamedPropRef::new(EntityPath::Parent, "uniforms"), None)).unwrap();
        document.set_property(&entity_id, "alpha", Pon::DependencyReference(NamedPropRef::new(EntityPath::Parent, "alpha"), None)).unwrap();

        for mesh_id in node.meshes() {
            document.set_property(&entity_id, "mesh", Pon::new_typed_pon("mesh_from_resource", Pon::String(self.get_mesh_key(*mesh_id as usize)))).unwrap();
        }

        for n in node.child_iter() {
            self.append_node_to_entity(n, &mut document, entity_id);
        }
    }

    fn mesh_from_assimp_mesh(&self, aimesh: Mesh) -> mesh::Mesh {
        let mut mesh = mesh::Mesh::new(mesh::Layout::position_texcoord_normal(), aimesh.num_vertices() as usize, aimesh.num_faces() as usize);
        let position_attr = mesh.layout.get_attribute("position").cloned().unwrap();
        let texcoord_attr = mesh.layout.get_attribute("texcoord").cloned().unwrap();
        let normal_attr = mesh.layout.get_attribute("normal").cloned().unwrap();
        for i in 0..aimesh.num_vertices() {
            let position = aimesh.get_vertex(i).unwrap();
            let mut texcoord = aimesh.get_texture_coord(0, i).unwrap();
            let normal = aimesh.get_normal(i).unwrap();
            mesh.write_to_attribute(&position_attr, i as usize, vec![position.x, position.y, position.z]);
            mesh.write_to_attribute(&texcoord_attr, i as usize, vec![texcoord.x, if self.invert_texcoord_y { 1.0 - texcoord.y } else { texcoord.y }]);
            mesh.write_to_attribute(&normal_attr, i as usize, vec![normal.x, normal.y, normal.z]);
        }
        for i in 0..aimesh.num_faces() {
            let face = aimesh.get_face(i).unwrap();
            mesh.element_data[(i*3 + 0) as usize] = face[0];
            mesh.element_data[(i*3 + 1) as usize] = face[1];
            mesh.element_data[(i*3 + 2) as usize] = face[2];
        }
        mesh
    }
    fn track_set_from_assimp_animation(&self, aianim: Animation) -> pa::TrackSet {
        let mut track_set = pa::TrackSet {
            tracks: vec![]
        };
        for i in 0..aianim.num_channels {
            let ainodeanim = aianim.get_node_anim(i as usize).unwrap();
            let duration = Duration::milliseconds((aianim.duration * 1000.0 / aianim.ticks_per_second) as i64);
            let duration_sec = duration.num_milliseconds() as f32 / 1000.0;

            let mut position_keys = vec![];
            for l in 0..ainodeanim.num_position_keys {
                let key = ainodeanim.get_position_key(l as usize).unwrap();
                position_keys.push(pa::Key(key.time as f32 / aianim.ticks_per_second as f32, pa::Animatable::new(vec![key.value.x, key.value.y, key.value.z])))
            }
            track_set.tracks.push(Box::new(pa::CurveTrack {
                curve: Box::new(pa::LinearKeyFrameCurve {
                    keys: position_keys
                }.to_discreet((duration_sec * 60.0) as usize, duration_sec)),
                offset: Duration::zero(),
                property: NamedPropRef::new(EntityPath::Search(Box::new(EntityPath::This), ainodeanim.node_name.as_ref().to_string()), "translation"),
                loop_type: pa::Loop::Forever,
                duration: duration.clone(),
                curve_time: pa::CurveTime::Absolute
            }));

            let mut rotation_keys = vec![];
            for l in 0..ainodeanim.num_rotation_keys {
                let key = ainodeanim.get_rotation_key(l as usize).unwrap();
                rotation_keys.push(pa::Key(key.time as f32 / aianim.ticks_per_second as f32, pa::Animatable::new(vec![key.velue.w, key.velue.x, key.velue.y, key.velue.z])))
            }
            track_set.tracks.push(Box::new(pa::CurveTrack {
                curve: Box::new(pa::LinearKeyFrameCurve {
                    keys: rotation_keys
                }.to_discreet((duration_sec * 60.0) as usize, duration_sec)),
                offset: Duration::zero(),
                property: NamedPropRef::new(EntityPath::Search(Box::new(EntityPath::This), ainodeanim.node_name.as_ref().to_string()), "rotation"),
                loop_type: pa::Loop::Forever,
                duration: duration,
                curve_time: pa::CurveTime::Absolute
            }));

            let mut scale_keys = vec![];
            for l in 0..ainodeanim.num_scaling_keys {
                let key = ainodeanim.get_scaling_key(l as usize).unwrap();
                scale_keys.push(pa::Key(key.time as f32 / aianim.ticks_per_second as f32, pa::Animatable::new(vec![key.value.x, key.value.y, key.value.z])))
            }
            track_set.tracks.push(Box::new(pa::CurveTrack {
                curve: Box::new(pa::LinearKeyFrameCurve {
                    keys: scale_keys
                }.to_discreet((duration_sec * 60.0) as usize, duration_sec)),
                offset: Duration::zero(),
                property: NamedPropRef::new(EntityPath::Search(Box::new(EntityPath::This), ainodeanim.node_name.as_ref().to_string()), "scale"),
                loop_type: pa::Loop::Forever,
                duration: duration,
                curve_time: pa::CurveTime::Absolute
            }));
        }
        track_set
    }
}

#[test]
fn test() {
    let asset = Asset3d::from_file(&Path::new("test_assets/Palmtree1.x"));
    let mut doc = Document::new();
    asset.add_resources_to_document(&mut doc);
    asset.append_to_document(&mut doc, None);
    let polySurface3 = doc.get_entity_by_name("polySurface3").unwrap();
    assert_eq!(
        doc.get_property(&polySurface3, "scale").unwrap().translate::<cgmath::Vector3<f32>>(&mut TranslateContext::empty()).unwrap(),
        cgmath::Vector3::new(1.0, 1.0, 1.0));
    // println!("{}", doc.to_string());
    // assert_eq!("5", "");
}

fn mat_into(mat: Matrix4x4) -> cgmath::Matrix4<f32> {
    cgmath::Matrix4::new(mat.a1, mat.b1, mat.c1, mat.d1,
                 mat.a2, mat.b2, mat.c2, mat.d2,
                 mat.a3, mat.b3, mat.c3, mat.d3,
                 mat.a4, mat.b4, mat.c4, mat.d4)
}
