//! Commands sent from the parent process to the worker process.
//!
//! Serialized as JSON over the child's **stdin** (one JSON object per line).
//!
//! ```text
//! {"cmd":"down","x":…,"y":…}
//! {"cmd":"up","pointer_id":…,"x":…,"y":…}
//! {"cmd":"move","pointer_id":…,"from_x":…,"from_y":…,"to_x":…,"to_y":…}
//! {"cmd":"shutdown"}
//! ```

use serde::{Deserialize, Serialize};

/// Command sent from the parent to the touch-injection worker.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "cmd")]
pub enum Cmd {
    /// Begin a new touch contact at (`x`, `y`).
    Down { x: i32, y: i32 },
    /// Release an existing touch contact.
    Up { pointer_id: u32, x: i32, y: i32 },
    /// Begin an interpolated move for an existing contact.
    Move {
        pointer_id: u32,
        from_x: i32,
        from_y: i32,
        to_x: i32,
        to_y: i32,
    },
    /// Graceful shutdown (all active contacts are released first).
    Shutdown,
}

/// Response sent from the worker back to the parent on **stdout**.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum WorkerResponse {
    /// Worker has initialized and is ready to accept commands.
    Ready,
    /// A pointer ID has been allocated for a new touch contact.
    Allocated { pointer_id: u32 },
}
