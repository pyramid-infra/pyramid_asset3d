#![feature(convert)]
#[macro_use]
extern crate pyramid;
extern crate asset3d_to_pml;
extern crate ppromise;
extern crate time;

use ppromise::*;
use pyramid::document::*;
use pyramid::pon::*;
use pyramid::interface::*;
use pyramid::system::*;
use asset3d_to_pml::*;

use std::collections::HashMap;
use std::collections::hash_map::Entry;
use std::path::Path;
use std::path::PathBuf;
use std::mem;

struct Asset {
    asset: Promise<Asset3d>,
    pending_scene_adds: Vec<EntityId>
}

impl Asset {
    fn new(async_runner: &AsyncRunner, root_path: PathBuf, pon: Pon) -> Asset {
        let filename = pon.translate::<&str>().unwrap();
        let path_buff = root_path.join(Path::new(filename));
        let path = path_buff.as_path();
        Asset {
            asset: Promise::resolved(Asset3d::from_file(&path)),
            pending_scene_adds: vec![]
        }
    }
    fn append_to_entity(&mut self, document: &mut Document, entity_id: &EntityId) {
        match self.asset.value().is_some() {
            true => self.asset.value().unwrap().append_to_document(document, Some(*entity_id)),
            false => self.pending_scene_adds.push(*entity_id)
        }
    }
    fn update(&mut self, document: &mut Document) {
        if self.pending_scene_adds.len() > 0 && self.asset.value().is_some() {
            let pending_scene_adds = mem::replace(&mut self.pending_scene_adds, vec![]);
            for entity_id in pending_scene_adds {
                self.asset.value().unwrap().append_to_document(document, Some(entity_id));
            }
        }
    }
}

pub struct Asset3dSubSystem {
    root_path: PathBuf,
    assets: HashMap<Pon, Asset>,
    async_runner: AsyncRunner
}

impl Asset3dSubSystem {
    pub fn new(root_path: PathBuf) -> Asset3dSubSystem {
        ::asset3d_to_pml::init_logging();
        Asset3dSubSystem {
            root_path: root_path,
            assets: HashMap::new(),
            async_runner: AsyncRunner::new_pooled(4)
        }
    }
}

impl ISubSystem for Asset3dSubSystem {
    fn on_property_value_change(&mut self, system: &mut System, prop_refs: &Vec<PropRef>) {
        let document = system.document_mut();
        for pr in prop_refs.iter().filter(|pr| pr.property_key == "directx_x") {
            let pn = document.get_property_value(&pr.entity_id, &pr.property_key.as_str()).unwrap().clone();
            match document.get_property_value(&pr.entity_id, "scene_loaded") {
                Ok(_) => {
                    println!("WARNING: Trying to change .x file on entity that's already been assigned a .x file once {:?}, skipping.", pr);
                    continue;
                },
                Err(_) => {}
            }

            match self.assets.entry(pn.clone()) {
                Entry::Occupied(o) => {
                    o.into_mut().append_to_entity(document, &pr.entity_id)
                },
                Entry::Vacant(v) => {
                    let file = Asset::new(&self.async_runner, self.root_path.clone(), pn.clone());
                    file.asset.value().unwrap().add_resources_to_document(document);
                    v.insert(file).append_to_entity(document, &pr.entity_id);
                }
            }
            document.set_property(&pr.entity_id, "scene_loaded", pn.clone()).unwrap();
        }
    }
    fn update(&mut self, system: &mut System) {
        self.async_runner.try_resolve_all();
        for (_, file) in self.assets.iter_mut() {
            file.update(system.document_mut());
        }
    }
}
