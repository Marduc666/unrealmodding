//! Enum property

use crate::property_prelude::*;
use unreal_asset_base::unversioned::properties::{
    array_property::UsmapArrayPropertyData,
    enum_property::UsmapEnumPropertyData,
    map_property::UsmapMapPropertyData,
    set_property::UsmapSetPropertyData,
};

/// Enum property
#[derive(FNameContainer, Debug, Clone, Default, PartialEq, Eq, Hash)]
pub struct EnumProperty {
    /// Name
    pub name: FName,
    /// Property ancestry
    pub ancestry: Ancestry,
    /// Property guid
    pub property_guid: Option<Guid>,
    /// Property duplication index
    pub duplication_index: i32,
    /// Enum type
    pub enum_type: Option<FName>,
    /// Inner type, used only with unversioned properties
    pub inner_type: Option<FName>,
    /// Enum value
    pub value: Option<FName>,
    /// Numeric value that no name was found for, used only with unversioned properties
    #[container_ignore]
    pub raw_value: Option<i64>,
}
impl_property_data_trait!(EnumProperty);

impl EnumProperty {
    /// Read an `EnumProperty` from an asset
    pub fn new<Reader: ArchiveReader<impl PackageIndexTrait>>(
        asset: &mut Reader,
        name: FName,
        ancestry: Ancestry,
        include_header: bool,
        _length: i64,
        duplication_index: i32,
    ) -> Result<Self, Error> {
        let mut enum_type: Option<FName> = None;
        let mut inner_type: Option<FName> = None;
        if asset.has_unversioned_properties() {
            // Look up the property info from the usmap
            let enum_data: Option<&UsmapEnumPropertyData> = asset
                .get_mappings()
                .and_then(|e| e.get_property(&name, &ancestry))
                .and_then(|e| {
                    // Direct EnumProperty
                    if let Some(data) = cast!(UsmapPropertyData, UsmapEnumPropertyData, &e.property_data) {
                        return Some(data);
                    }
                    // Array element — extract inner EnumProperty
                    if let UsmapPropertyData::UsmapArrayPropertyData(ref arr) = e.property_data {
                        if let UsmapPropertyData::UsmapEnumPropertyData(ref inner) = *arr.inner_type {
                            return Some(inner);
                        }
                    }
                    // Map value — extract inner EnumProperty
                    if let UsmapPropertyData::UsmapMapPropertyData(ref map) = e.property_data {
                        if let UsmapPropertyData::UsmapEnumPropertyData(ref inner) = *map.value_type {
                            return Some(inner);
                        }
                    }
                    // Set element — extract inner EnumProperty
                    if let UsmapPropertyData::UsmapSetPropertyData(ref set) = e.property_data {
                        if let UsmapPropertyData::UsmapEnumPropertyData(ref inner) = *set.inner_type {
                            return Some(inner);
                        }
                    }
                    None
                });

            if let Some(enum_data) = enum_data {
                let enum_ty = FName::new_dummy(enum_data.name.clone(), 0);
                let inner_ty_str = enum_data.inner_property.get_property_type().to_string();
                let inner_ty = FName::new_dummy(inner_ty_str.clone(), 0);

                // For numeric inner types, read the raw value and map to enum name
                let numeric_value: Option<i64> = match inner_ty_str.as_str() {
                    "ByteProperty" => Some(asset.read_u8()? as i64),
                    "UInt16Property" => Some(asset.read_u16::<LE>()? as i64),
                    "UInt32Property" => Some(asset.read_u32::<LE>()? as i64),
                    "UInt64Property" => Some(asset.read_u64::<LE>()? as i64),
                    "Int8Property" => Some(asset.read_i8()? as i64),
                    "Int16Property" => Some(asset.read_i16::<LE>()? as i64),
                    "IntProperty" => Some(asset.read_i32::<LE>()? as i64),
                    "Int64Property" => Some(asset.read_i64::<LE>()?),
                    _ => None,
                };

                if let Some(enum_index) = numeric_value {
                    let mappings = asset.get_mappings().unwrap();
                    let (info, values) = enum_ty
                        .get_content(|ty| {
                            Some((
                                mappings.enum_map.get_by_key(ty)?,
                                mappings.enum_values.get_by_key(ty)?,
                            ))
                        })
                        .ok_or_else(|| {
                            Error::invalid_file(enum_ty.get_content(|ty| {
                                "Missing unversioned info for: ".to_string() + ty
                            }))
                        })?;
                    let value = values
                        .iter()
                        .position(|value| *value == enum_index)
                        .map(|index| FName::new_dummy(info[index].clone(), 0));
                    let raw_value = value.is_none().then_some(enum_index);

                    return Ok(EnumProperty {
                        name,
                        ancestry,
                        property_guid: None,
                        duplication_index,
                        enum_type: Some(enum_ty),
                        inner_type: Some(inner_ty),
                        value,
                        raw_value,
                    });
                }

                enum_type = Some(enum_ty);
                inner_type = Some(inner_ty);
            }
        }

        let property_guid = match include_header {
            true => {
                enum_type = Some(asset.read_fname()?);
                asset.read_property_guid()?
            }
            false => None,
        };
        let value = asset.read_fname()?;

        Ok(EnumProperty {
            name,
            ancestry,
            property_guid,
            duplication_index,
            enum_type,
            inner_type,
            value: Some(value),
            raw_value: None,
        })
    }
}

impl PropertyTrait for EnumProperty {
    fn write<Writer: ArchiveWriter<impl PackageIndexTrait>>(
        &self,
        asset: &mut Writer,
        include_header: bool,
    ) -> Result<usize, Error> {
        let inner_size = self.inner_type.as_ref().and_then(|ty| {
            ty.get_content(|ty| match ty {
                "ByteProperty" | "Int8Property" => Some(size_of::<u8>()),
                "UInt16Property" | "Int16Property" => Some(size_of::<u16>()),
                "UInt32Property" | "IntProperty" => Some(size_of::<u32>()),
                "UInt64Property" | "Int64Property" => Some(size_of::<u64>()),
                _ => None,
            })
        });
        if let (true, Some(inner_size)) = (asset.has_unversioned_properties(), inner_size) {
            self.enum_type
                .as_ref()
                .ok_or_else(|| {
                    Error::no_data("enum_type is None on an unversioned property".to_string())
                })?
                .get_content(|enum_type| {
                    let mappings = asset
                        .get_mappings()
                        .ok_or_else(PropertyError::no_mappings)?;
                    let missing = || {
                        Error::invalid_file("Missing unversioned info for: ".to_string() + enum_type)
                    };
                    let info = mappings.enum_map.get_by_key(enum_type).ok_or_else(missing)?;
                    let values = mappings
                        .enum_values
                        .get_by_key(enum_type)
                        .ok_or_else(missing)?;

                    let enum_value = match self.value.as_ref() {
                        Some(value) => info
                            .iter()
                            .position(|e| value == e.as_str())
                            .map(|index| values[index])
                            .ok_or_else(missing)?,
                        None => self.raw_value.unwrap_or(-1),
                    };

                    asset.write_all(&enum_value.to_le_bytes()[..inner_size])?;
                    Ok::<(), Error>(())
                })?;
            return Ok(inner_size);
        }

        if include_header {
            asset.write_fname(
                self.enum_type
                    .as_ref()
                    .ok_or_else(PropertyError::headerless)?,
            )?;
            asset.write_property_guid(self.property_guid.as_ref())?;
        }
        asset.write_fname(self.value.as_ref().unwrap())?;

        Ok(size_of::<i32>() * 2)
    }
}
