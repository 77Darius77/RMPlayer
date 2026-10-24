//! Rendu vidéo — egui Image (simplifié)
//! 
//! Pour l'instant, le rendu vidéo est géré par egui via TextureId.
//! Le pipeline wgpu/Vulkan sera implémenté dans une prochaine version.

use std::sync::{Arc, Mutex};

/// État du rendu vidéo
pub struct VideoRenderer {
    /// Données vidéo actuelles (RGBA)
    current_frame: Arc<Mutex<Option<Vec<u8>>>>,
    /// Dimensions de la frame
    dimensions: Arc<Mutex<(u32, u32)>>,
}

impl VideoRenderer {
    /// Créer un nouveau renderer
    pub fn new() -> Self {
        tracing::info!("Rendu vidéo initialisé (mode egui)");
        Self {
            current_frame: Arc::new(Mutex::new(None)),
            dimensions: Arc::new(Mutex::new((0, 0))),
        }
    }
    
    /// Mettre à jour la frame vidéo actuelle
    pub fn update_frame(&self, data: Vec<u8>, width: u32, height: u32) {
        let mut frame = self.current_frame.lock().unwrap();
        *frame = Some(data);
        let mut dims = self.dimensions.lock().unwrap();
        *dims = (width, height);
    }
    
    /// Obtenir la frame vidéo actuelle
    pub fn get_frame(&self) -> Option<Vec<u8>> {
        self.current_frame.lock().unwrap().clone()
    }
    
    /// Obtenir les dimensions de la frame
    pub fn get_dimensions(&self) -> (u32, u32) {
        *self.dimensions.lock().unwrap()
    }
}
