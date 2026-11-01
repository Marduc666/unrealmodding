//! Schemas for blueprint classes and structs
//!
//! Mappings dumped from a running game only have what was loaded at the time, so most blueprints
//! are missing and the actors using them can't be read. Their properties are stored in the
//! packages they were cooked to though, so the schemas can be rebuilt from those

use std::{
    collections::{HashMap, HashSet},
    fs,
    io::{Cursor, Read, Seek},
    path::{Path, PathBuf},
};

use unreal_asset_base::{
    enums::EArrayDim,
    types::{PackageIndex, PackageIndexTrait},
    unversioned::{
        properties::{
            array_property::UsmapArrayPropertyData, enum_property::UsmapEnumPropertyData,
            map_property::UsmapMapPropertyData, set_property::UsmapSetPropertyData,
            shallow_property::UsmapShallowPropertyData, struct_property::UsmapStructPropertyData,
            EPropertyType, UsmapProperty, UsmapPropertyData,
        },
        Usmap, UsmapSchema,
    },
};
use unreal_asset_exports::{
    properties::fproperty::{FGenericProperty, FProperty},
    struct_export::StructExport,
    ExportBaseTrait,
};

use crate::{cast, containers::IndexedMap, engine_version::EngineVersion, Asset, Export};

/// Finds the files of packages in a folder of loose cooked content
pub struct LooseContent {
    /// Content folder of each mount point
    mounts: HashMap<String, PathBuf>,
}

impl LooseContent {
    /// Finds the mount points below `root` which has a folder for the game and for the engine
    pub fn new(root: &Path, game: &str) -> Self {
        let mut mounts = HashMap::new();
        mounts.insert("Game".to_string(), root.join(game).join("Content"));
        mounts.insert("Engine".to_string(), root.join("Engine").join("Content"));

        let mut pending = vec![
            root.join(game).join("Plugins"),
            root.join(game).join("Mods"),
            root.join("Engine").join("Plugins"),
        ];
        while let Some(dir) = pending.pop() {
            let Ok(entries) = fs::read_dir(&dir) else {
                continue;
            };
            let dirs = entries
                .flatten()
                .map(|entry| entry.path())
                .filter(|path| path.is_dir());
            for path in dirs {
                let name = path.file_name().map(|e| e.to_string_lossy().into_owned());
                match name.as_deref() {
                    Some("Content") => {
                        if let Some(plugin) = dir.file_name() {
                            mounts.insert(plugin.to_string_lossy().into_owned(), path);
                        }
                    }
                    _ => pending.push(path),
                }
            }
        }

        LooseContent { mounts }
    }

    /// Reads the export map and exports of a package from its path, like `/Game/Maps/Map`
    pub fn load(&self, package: &str) -> Option<(Vec<u8>, Option<Vec<u8>>)> {
        let (mount, path) = package.strip_prefix('/')?.split_once('/')?;
        let file = self.mounts.get(mount)?.join(path);
        let data = ["uasset", "umap"]
            .iter()
            .find_map(|ext| fs::read(file.with_extension(ext)).ok())?;
        Some((data, fs::read(file.with_extension("uexp")).ok()))
    }
}

/// The package holding an import
fn package_of<C: Read + Seek>(asset: &Asset<C>, mut index: PackageIndex) -> Option<String> {
    loop {
        let import = asset.get_import(index)?;
        if !import.outer_index.is_import() {
            return Some(import.object_name.get_owned_content());
        }
        index = import.outer_index;
    }
}

/// The name of the object at a package index
fn object_name<C: Read + Seek>(asset: &Asset<C>, index: PackageIndex) -> Option<String> {
    match index.is_import() {
        true => asset
            .get_import(index)
            .map(|import| import.object_name.get_owned_content()),
        false => asset
            .asset_data
            .get_export(index)
            .map(|export| export.get_base_export().object_name.get_owned_content()),
    }
}

/// The part of a property that all kinds of properties have
fn generic_property(property: &FProperty) -> &FGenericProperty {
    match property {
        FProperty::FGenericProperty(e) => e,
        FProperty::FEnumProperty(e) => &e.generic_property,
        FProperty::FArrayProperty(e) => &e.generic_property,
        FProperty::FSetProperty(e) => &e.generic_property,
        FProperty::FObjectProperty(e) => &e.generic_property,
        FProperty::FSoftObjectProperty(e) => &e.generic_property,
        FProperty::FClassProperty(e) => &e.generic_property,
        FProperty::FSoftClassProperty(e) => &e.generic_property,
        FProperty::FDelegateProperty(e) => &e.generic_property,
        FProperty::FMulticastDelegateProperty(e) => &e.generic_property,
        FProperty::FMulticastInlineDelegateProperty(e) => &e.generic_property,
        FProperty::FInterfaceProperty(e) => &e.generic_property,
        FProperty::FMapProperty(e) => &e.generic_property,
        FProperty::FBoolProperty(e) => &e.generic_property,
        FProperty::FByteProperty(e) => &e.generic_property,
        FProperty::FStructProperty(e) => &e.generic_property,
        FProperty::FNumericProperty(e) => &e.generic_property,
    }
}

/// The usmap equivalent of the serialized type of a property
fn shallow_type(property: &FProperty) -> EPropertyType {
    // only generic properties keep their type, the rest of them are implied by their variant
    let serialized = match property {
        FProperty::FObjectProperty(_) | FProperty::FClassProperty(_) => "ObjectProperty".into(),
        FProperty::FSoftObjectProperty(_) | FProperty::FSoftClassProperty(_) => {
            "SoftObjectProperty".into()
        }
        FProperty::FDelegateProperty(_) => "DelegateProperty".into(),
        FProperty::FMulticastDelegateProperty(_) => "MulticastDelegateProperty".into(),
        FProperty::FMulticastInlineDelegateProperty(_) => "MulticastInlineDelegateProperty".into(),
        FProperty::FInterfaceProperty(_) => "InterfaceProperty".into(),
        FProperty::FBoolProperty(_) => "BoolProperty".into(),
        FProperty::FByteProperty(_) => "ByteProperty".into(),
        _ => generic_property(property)
            .serialized_type
            .as_ref()
            .map(|e| e.get_owned_content())
            .unwrap_or_default(),
    };
    (0..=u8::MAX)
        .filter_map(|e| EPropertyType::try_from(e).ok())
        .find(|ty| ty.to_string() == serialized)
        .unwrap_or(EPropertyType::Unknown)
}

/// Converts a property to its usmap equivalent, adding what it depends on to `packages`
fn convert<C: Read + Seek>(
    asset: &Asset<C>,
    property: &FProperty,
    mappings: &mut Usmap,
    packages: &mut Vec<String>,
) -> UsmapPropertyData {
    let inner = |property: &FProperty, mappings: &mut Usmap, packages: &mut Vec<String>| {
        Box::new(convert(asset, property, mappings, packages))
    };
    match property {
        FProperty::FArrayProperty(e) => UsmapArrayPropertyData {
            inner_type: inner(&e.inner, mappings, packages),
        }
        .into(),
        FProperty::FSetProperty(e) => UsmapSetPropertyData {
            inner_type: inner(&e.element_prop, mappings, packages),
        }
        .into(),
        FProperty::FMapProperty(e) => UsmapMapPropertyData {
            inner_type: inner(&e.key_prop, mappings, packages),
            value_type: inner(&e.value_prop, mappings, packages),
        }
        .into(),
        FProperty::FStructProperty(e) => {
            let struct_type = object_name(asset, e.struct_value).unwrap_or_default();
            if mappings.schemas.get_by_key(&struct_type).is_none() {
                packages.extend(package_of(asset, e.struct_value));
            }
            UsmapStructPropertyData { struct_type }.into()
        }
        FProperty::FEnumProperty(e) => {
            let name = object_name(asset, e.enum_value).unwrap_or_default();
            // the values of blueprint enums aren't needed as unknown ones are kept as numbers
            if mappings.enum_map.get_by_key(&name).is_none() {
                mappings.enum_map.insert(name.clone(), Vec::new());
                mappings.enum_values.insert(name.clone(), Vec::new());
            }
            UsmapEnumPropertyData {
                inner_property: inner(&e.underlying_prop, mappings, packages),
                name,
            }
            .into()
        }
        _ => UsmapShallowPropertyData {
            property_type: shallow_type(property),
        }
        .into(),
    }
}

/// Adds the schema of a class or struct, returns the packages it needs that aren't mapped yet
fn add_schema<C: Read + Seek>(
    asset: &Asset<C>,
    export: &StructExport<PackageIndex>,
    mappings: &mut Usmap,
    packages: &mut Vec<String>,
) {
    let name = export
        .normal_export
        .base_export
        .object_name
        .get_owned_content();
    if mappings.schemas.get_by_key(&name).is_some() {
        return;
    }

    let super_type = object_name(asset, export.super_struct).unwrap_or_default();
    if !super_type.is_empty() && mappings.schemas.get_by_key(&super_type).is_none() {
        packages.extend(package_of(asset, export.super_struct));
    }

    let mut properties = IndexedMap::with_capacity(export.loaded_properties.len());
    let mut prop_count = 0;
    for property in &export.loaded_properties {
        let generic = generic_property(property);
        // properties can differ by nothing but their number
        let name = match generic.name.get_number() {
            number if number > 0 => {
                format!("{}_{}", generic.name.get_owned_content(), number - 1)
            }
            _ => generic.name.get_owned_content(),
        };
        let array_size = match generic.array_dim {
            EArrayDim::CArray => 2,
            _ => 1,
        };
        let property_data = convert(asset, property, mappings, packages);
        for array_index in 0..array_size {
            properties.insert(
                (name.clone(), array_index as u32),
                UsmapProperty {
                    name: name.clone(),
                    schema_index: prop_count,
                    array_size,
                    array_index: array_index as u16,
                    property_data: property_data.clone(),
                },
            );
            prop_count += 1;
        }
    }

    mappings.schemas.insert(
        name.clone(),
        UsmapSchema {
            name,
            super_type,
            prop_count,
            module_path: None,
            properties,
        },
    );
}

/// Adds the schemas of every class and struct of an asset
fn add_schemas<C: Read + Seek>(asset: &Asset<C>, mappings: &mut Usmap, packages: &mut Vec<String>) {
    for export in &asset.asset_data.exports {
        if let Some(class) = cast!(Export, ClassExport, export) {
            add_schema(asset, &class.struct_export, mappings, packages);
        } else if let Some(user_struct) = cast!(Export, UserDefinedStructExport, export) {
            add_schema(asset, &user_struct.struct_export, mappings, packages);
        }
    }
}

/// Adds the schemas of all the blueprints an asset uses which `mappings` doesn't have, returns
/// how many were added. Only exports read after this can use them so read the asset again
pub fn pull_schemas<C: Read + Seek>(
    asset: &Asset<C>,
    engine_version: EngineVersion,
    mappings: &mut Usmap,
    visited: &mut HashSet<String>,
    load: impl Fn(&str) -> Option<(Vec<u8>, Option<Vec<u8>>)>,
) -> usize {
    let schemas_before = mappings.schemas.len();

    let mut packages = Vec::new();
    add_schemas(asset, mappings, &mut packages);
    for export in &asset.asset_data.exports {
        let class = export.get_base_export().class_index;
        let is_mapped = object_name(asset, class)
            .is_some_and(|name| mappings.schemas.get_by_key(&name).is_some());
        if class.is_import() && !is_mapped {
            packages.extend(package_of(asset, class));
        }
    }

    while let Some(package) = packages.pop() {
        if !visited.insert(package.clone()) {
            continue;
        }
        let Some((data, bulk)) = load(&package) else {
            continue;
        };
        if let Ok(dependency) = Asset::new(
            Cursor::new(data),
            bulk.map(Cursor::new),
            engine_version,
            Some(mappings.clone()),
        ) {
            add_schemas(&dependency, mappings, &mut packages);
        }
    }

    mappings.schemas.len() - schemas_before
}
