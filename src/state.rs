use bevy::prelude::*;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, States)]
pub enum GameState {
    Playing,
    Paused,
    GameOver,
}
