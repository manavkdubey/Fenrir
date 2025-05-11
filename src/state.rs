use bevy::prelude::*;
use bevy_state::prelude::*;
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, States)]
pub enum GameState {
    Loading,
    Menu,
    Playing,
    Paused,
    GameOver,
    Cleanup,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum GameCommand {
    StartGame,
    Pause,
    Resume,
    Restart,
    Quit,
}

pub struct GameScene;
fn initialize_game_state(mut next_state: ResMut<NextState<GameState>>) {
    next_state.set(GameState::Loading);
}
