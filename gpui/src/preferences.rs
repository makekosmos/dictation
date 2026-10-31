use serde_json::{json, Value};

const FILE_NAME: &str = "dictation-gpui-settings.json";

#[derive(Clone, Copy, Debug)]
pub(crate) struct Preferences {
    pub sounds_enabled: bool,
    pub sound_volume: f32,
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            sounds_enabled: true,
            sound_volume: 1.0,
        }
    }
}

impl Preferences {
    pub fn load() -> Self {
        let Ok(dir) = mundus_gpui_kit::engine::data_dir() else {
            return Self::default();
        };
        let Ok(value) = std::fs::read(dir.join(FILE_NAME)) else {
            return Self::default();
        };
        let Ok(value) = serde_json::from_slice::<Value>(&value) else {
            return Self::default();
        };
        Self {
            sounds_enabled: value["soundsEnabled"].as_bool().unwrap_or(true),
            sound_volume: value["soundVolume"]
                .as_f64()
                .map(|volume| volume as f32)
                .unwrap_or(1.0)
                .clamp(0.0, 1.0),
        }
    }

    pub fn save(self) -> Result<(), String> {
        let dir = mundus_gpui_kit::engine::data_dir().map_err(|error| error.to_string())?;
        std::fs::create_dir_all(&dir).map_err(|error| error.to_string())?;
        let value = json!({
            "soundsEnabled": self.sounds_enabled,
            "soundVolume": self.sound_volume.clamp(0.0, 1.0),
        });
        std::fs::write(
            dir.join(FILE_NAME),
            serde_json::to_vec_pretty(&value).map_err(|error| error.to_string())?,
        )
        .map_err(|error| error.to_string())
    }
}
