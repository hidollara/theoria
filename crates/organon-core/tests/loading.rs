//! No XML or filesystem is needed to load and analyze host-owned modules.
use organon_core::{
    Error,
    loader::{self, Sources},
    semantic,
    syntax::{
        DeclareElement, Definition, Element, ElementMember, Export, Import, ImportSource, Item,
        Module, Provide, Use,
    },
    types::{Port, TypeName, ValueType},
};
use std::collections::BTreeMap;

struct Memory {
    modules: BTreeMap<u32, Module>,
    links: BTreeMap<(u32, String), u32>,
    reads: Vec<u32>,
}
impl Sources for Memory {
    type Id = u32;
    fn resolve(&mut self, from: &u32, import: &ImportSource) -> Result<u32, Error> {
        self.links
            .get(&(*from, import.as_str().into()))
            .copied()
            .ok_or_else(|| Error(format!("unknown import {:?}", import.as_str())))
    }
    fn read(&mut self, id: &u32) -> Result<Module, Error> {
        self.reads.push(*id);
        self.modules
            .remove(id)
            .ok_or_else(|| Error("missing or already read module".into()))
    }
}
fn export(name: &str) -> Item {
    Item::Export(Export {
        name: name.parse().unwrap(),
        alias: None,
    })
}
fn import(name: &str) -> Item {
    Item::Import(Import {
        from: ImportSource::Dependency(name.into()),
    })
}
fn memory() -> Memory {
    let library = Module {
        items: vec![
            Item::Definition(Definition::DeclareElement(DeclareElement {
                name: "Number".parse().unwrap(),
                tag: false,
                children: None,
                members: vec![],
            })),
            export("Number"),
        ],
    };
    let entry = Module {
        items: vec![
            Item::Use(Use {
                from: ImportSource::Dependency("numbers".into()),
                name: "Number".parse().unwrap(),
                alias: Some("First".parse().unwrap()),
            }),
            Item::Use(Use {
                from: ImportSource::Relative("./numbers".into()),
                name: "Number".parse().unwrap(),
                alias: Some("Second".parse().unwrap()),
            }),
            Item::Definition(Definition::Element(Element {
                name: "PassThrough".parse().unwrap(),
                children: None,
                members: vec![
                    ElementMember::Input(Port {
                        name: "value".parse().unwrap(),
                        ty: ValueType {
                            kind: TypeName::parse("First").unwrap(),
                            errors: vec![],
                        },
                    }),
                    ElementMember::Provide(Provide {
                        name: "result".parse().unwrap(),
                        reference: "value".parse().unwrap(),
                    }),
                ],
            })),
            export("First"),
            export("Second"),
            export("PassThrough"),
        ],
    };
    Memory {
        modules: BTreeMap::from([(0, entry), (1, library)]),
        links: BTreeMap::from([((0, "numbers".into()), 1), ((0, "./numbers".into()), 1)]),
        reads: vec![],
    }
}

#[test]
fn memory_modules_share_identity_and_produce_contracts_and_bodies() {
    let mut sources = memory();
    let original = sources.modules[&0].clone();
    let loaded = loader::load(0, &mut sources).unwrap();
    assert_eq!(sources.reads, [0, 1]);
    assert_eq!(loaded.source_order, [1, 0]);
    assert_eq!(loaded.sources[&0], original);
    assert_eq!(loaded.public["First"], loaded.public["Second"]);
    assert_eq!(loaded.origins[&loaded.public["First"]].source, 1);

    let program = semantic::resolve(&loaded.definitions).unwrap();
    assert_eq!(program.contracts.len(), 2);
    assert_eq!(program.bodies.len(), 1);
    let contract = &program.contracts["PassThrough"];
    assert_eq!(&contract.inputs[0].ty.kind, contract.outputs[0].ty.kind());
    assert!(matches!(
        contract.outputs[0].ty,
        semantic::OutputType::Inferred(_)
    ));
    assert_eq!(
        contract.outputs[0].ty.kind().spelling(),
        loaded.public["First"]
    );
    let body = &program.bodies[0];
    assert_eq!(body.name, "PassThrough");
    assert_eq!(body.outputs[0].binding, "value");
}

#[test]
fn cycles_and_unresolved_imports_are_reported_for_virtual_sources() {
    let mut missing = memory();
    missing.links.clear();
    assert!(
        loader::load(0, &mut missing)
            .unwrap_err()
            .to_string()
            .contains("unknown import")
    );

    let mut cyclic = Memory {
        modules: BTreeMap::from([
            (
                0,
                Module {
                    items: vec![import("next")],
                },
            ),
            (
                1,
                Module {
                    items: vec![import("back")],
                },
            ),
        ]),
        links: BTreeMap::from([((0, "next".into()), 1), ((1, "back".into()), 0)]),
        reads: vec![],
    };
    assert!(
        loader::load(0, &mut cyclic)
            .unwrap_err()
            .to_string()
            .contains("cyclic imports")
    );
    assert_eq!(cyclic.reads, [0, 1]);
}
