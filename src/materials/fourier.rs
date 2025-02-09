//std
use std::sync::Arc;
// pbrt
use core::api::BsdfState;
use core::interaction::SurfaceInteraction;
use core::material::{Material, TransportMode};
use core::paramset::TextureParams;
use core::pbrt::Float;
use core::reflection::{Bsdf, Bxdf, FourierBSDF, FourierBSDFTable};
use core::texture::Texture;

// see fourier.h

pub struct FourierMaterial {
    pub bsdf_table: Arc<FourierBSDFTable>,
    pub bump_map: Option<Arc<Texture<Float> + Sync + Send>>,
}

impl FourierMaterial {
    pub fn new(
        bsdf_table: Arc<FourierBSDFTable>,
        bump_map: Option<Arc<Texture<Float> + Sync + Send>>,
    ) -> Self {
        FourierMaterial {
            bump_map: bump_map,
            bsdf_table: bsdf_table,
        }
    }
    pub fn create(
        mp: &mut TextureParams,
        bsdf_state: &mut BsdfState,
    ) -> Arc<Material + Send + Sync> {
        let bump_map: Option<Arc<Texture<Float> + Send + Sync>> =
            mp.get_float_texture_or_null("bumpmap");
        let bsdffile: String = mp.find_filename("bsdffile", String::new());
        if let Some(bsdf_table) = bsdf_state.loaded_bsdfs.get(&bsdffile.clone()) {
            // use the BSDF table found
            Arc::new(FourierMaterial::new(bsdf_table.clone(), bump_map))
        } else {
            // read BSDF table from file
            let mut bsdf_table: FourierBSDFTable = FourierBSDFTable::default();
            println!(
                "reading {:?} returns {}",
                bsdffile,
                bsdf_table.read(&bsdffile)
            );
            let bsdf_table_arc: Arc<FourierBSDFTable> = Arc::new(bsdf_table);
            // TODO: bsdf_state.loaded_bsdfs.insert(bsdffile.clone(), bsdf_table_arc.clone());
            Arc::new(FourierMaterial::new(bsdf_table_arc.clone(), bump_map))
        }
    }
}

impl Material for FourierMaterial {
    fn compute_scattering_functions(
        &self,
        si: &mut SurfaceInteraction,
        // arena: &mut Arena,
        mode: TransportMode,
        _allow_multiple_lobes: bool,
        _material: Option<Arc<Material + Send + Sync>>,
    ) {
        if let Some(ref bump) = self.bump_map {
            Self::bump(bump, si);
        }
        let mut bxdfs: Vec<Arc<Bxdf + Send + Sync>> = Vec::new();
        bxdfs.push(Arc::new(FourierBSDF::new(self.bsdf_table.clone(), mode)));
        si.bsdf = Some(Arc::new(Bsdf::new(si, 1.0, bxdfs)));
    }
}
