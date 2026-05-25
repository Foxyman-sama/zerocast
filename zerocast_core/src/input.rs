use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum RemoteInput {
  MouseMove { x: f32, y: f32 }, // Normalized coordinates: 0.0 to 1.0
  MouseDown { button: String },
  MouseUp { button: String },
  KeyPress { key_code: u16 },
  KeyRelease { key_code: u16 },
  Ping { client_time: u64 },
}
