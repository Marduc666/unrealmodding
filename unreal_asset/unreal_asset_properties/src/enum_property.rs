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
                    let info = enum_ty
                        .get_content(|ty| asset.get_mappings().unwrap().enum_map.get_by_key(ty))
                        .ok_or_else(|| {
                            Error::invalid_file(enum_ty.get_content(|ty| {
                                "Missing unversioned info for: ".to_string() + ty
                            }))
                        })?;
                    let value = if enum_index < 0 || enum_index as usize >= info.len() {
                        None
                    } else {
                        Some(FName::new_dummy(info[enum_index as usize].clone(), 0))
                    };

                    return Ok(EnumProperty {
                        name,
                        ancestry,
                        property_guid: None,
                        duplication_index,
                        enum_type: Some(enum_ty),
                        inner_type: Some(inner_ty),
                        value,
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
        })
    }
}

impl PropertyTrait for EnumProperty {
    fn write<Writer: ArchiveWriter<impl PackageIndexTrait>>(
        &self,
        asset: &mut Writer,
        include_header: bool,
    ) -> Result<usize, Error> {
        if asset.has_unversioned_properties()
            && self
                .inner_type
                .as_ref()
                .map(|e| e == "ByteProperty")
                .unwrap_or(false)
        {
            self.enum_type
                .as_ref()
                .ok_or_else(|| {
                    Error::no_data("enum_type is None on an unversioned property".to_string())
                })?
                .get_content(|enum_type| {
                    let info = asset
                        .get_mappings()
                        .ok_or_else(PropertyError::no_mappings)?
                        .enum_map
                        .get_by_key(enum_type)
                        .ok_or_else(|| {
                            Error::invalid_file(
                                "Missing unversioned info for: ".to_string() + enum_type,
                            )
                        })?;

                    let enum_index = match self.value.as_ref() {
                        Some(value) => info
                            .iter()
                            .enumerate()
                            .find(|(_, e)| value == e.as_str())
                            .map(|(index, _)| index as u8)
                            .ok_or_else(|| {
                                Error::invalid_file(
                                    "Missing unversioned info for: ".to_string() + enum_type,
                                )
                            })?,
                        None => u8::MAX,
                    };

                    asset.write_u8(enum_index)?;
                    Ok::<(), Error>(())
                })?;
            return Ok(size_of::<u8>());
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
