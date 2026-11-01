//! Normal export

use unreal_asset_base::{
    flags::EObjectFlags,
    object_version::ObjectVersionUE5,
    reader::{ArchiveReader, ArchiveWriter},
    types::PackageIndexTrait,
    unversioned::{header::UnversionedHeader, Ancestry},
    Error, FNameContainer,
};
use unreal_asset_properties::{generate_unversioned_header, Property};

use crate::BaseExport;
use crate::{ExportBaseTrait, ExportNormalTrait, ExportTrait};
use byteorder::{ReadBytesExt, WriteBytesExt, LE};

/// Normal export
///
/// This export is usually the base export for all other exports
#[derive(FNameContainer, Debug, Clone, PartialEq, Eq, Hash)]
pub struct NormalExport<Index: PackageIndexTrait> {
    /// Base export
    pub base_export: BaseExport<Index>,
    /// Extra data
    pub extras: Vec<u8>,
    /// Properties
    pub properties: Vec<Property>,
}

impl<Index: PackageIndexTrait> ExportNormalTrait<Index> for NormalExport<Index> {
    fn get_normal_export(&'_ self) -> Option<&'_ NormalExport<Index>> {
        Some(self)
    }

    fn get_normal_export_mut(&'_ mut self) -> Option<&'_ mut NormalExport<Index>> {
        Some(self)
    }
}

impl<Index: PackageIndexTrait> ExportBaseTrait<Index> for NormalExport<Index> {
    fn get_base_export(&'_ self) -> &'_ BaseExport<Index> {
        &self.base_export
    }

    fn get_base_export_mut(&'_ mut self) -> &'_ mut BaseExport<Index> {
        &mut self.base_export
    }
}

impl<Index: PackageIndexTrait> NormalExport<Index> {
    /// Read a `NormalExport` from an asset
    pub fn from_base<Reader: ArchiveReader<Index>>(
        base: &BaseExport<Index>,
        asset: &mut Reader,
    ) -> Result<Self, Error> {
        let object_version_ue5 = asset.get_object_version_ue5();
        let is_cdo = base.object_flags.contains(EObjectFlags::RF_CLASS_DEFAULT_OBJECT);

        // UE5.4+ leading 4 null bytes check (UAssetAPI NormalExport.Read)
        // Only applies for versions between DATA_RESOURCES and ASSETREGISTRY_PACKAGEBUILDDEPENDENCIES
        if object_version_ue5 > ObjectVersionUE5::DATA_RESOURCES
            && object_version_ue5 < ObjectVersionUE5::ASSETREGISTRY_PACKAGEBUILDDEPENDENCIES
            && !is_cdo
        {
            let dummy = asset.read_i32::<LE>()?;
            if dummy != 0 {
                // Not null bytes — seek back, this is property data
                asset.seek(std::io::SeekFrom::Current(-4))?;
            }
        }

        let mut properties = Vec::new();

        let mut unversioned_header = UnversionedHeader::new(asset)?;
        let ancestry = Ancestry::new(base.get_class_type_for_ancestry(asset));
        while let Some(e) =
            Property::new(asset, ancestry.clone(), unversioned_header.as_mut(), true)?
        {
            properties.push(e);
        }

        // Read ObjectGuid (UAssetAPI: ReadBooleanInt reads i32, not byte)
        if !is_cdo {
            let has_guid = asset.read_i32::<LE>()?;
            if has_guid != 0 {
                let _guid = asset.read_guid()?;
            }
        }

        Ok(NormalExport {
            base_export: base.clone(),
            extras: Vec::new(),

            properties,
        })
    }
}

impl<Index: PackageIndexTrait> ExportTrait<Index> for NormalExport<Index> {
    fn write<Writer: ArchiveWriter<Index>>(&self, asset: &mut Writer) -> Result<(), Error> {
        let (unversioned_header, sorted_properties) = match generate_unversioned_header(
            asset,
            &self.properties,
            &self.base_export.get_class_type_for_ancestry(asset),
        )? {
            Some((a, b)) => (Some(a), Some(b)),
            None => (None, None),
        };

        if let Some(unversioned_header) = unversioned_header {
            unversioned_header.write(asset)?;
        }

        let properties = sorted_properties.as_ref().unwrap_or(&self.properties);

        for entry in properties.iter() {
            Property::write(entry, asset, true)?;
        }
        if !asset.has_unversioned_properties() {
            let none = asset.get_name_map().get_mut().add_fname("None");
            asset.write_fname(&none)?;
        }

        Ok(())
    }
}
