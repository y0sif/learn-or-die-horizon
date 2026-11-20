//! Learn or Die (Horizon port): the game-independent teacher (`core`) wired to the game's hooks
//! (`game`). Ported from Learn or Die by Laren, All Rights Reserved, with permission.
//!
//! The teacher lives on the client, as in the original: detection, his voice, the HUD and the
//! question screen all run there, so he works on any server. The integrated server's half hands
//! the client the world seed for the progress file's world key, and applies an answer's
//! consequences to the host's own player (`game::server`).
//!
//! The client hooks act only for this window's own game: a split-screen player's game, in this
//! process too, would otherwise drive the same teacher with its own player (`game::hosting`).
use mod_api::{
    AddClientReloadListenersEvent, ClientHost, ClientPlayerLoggingOutEvent, ClientTickPostEvent,
    Flow, GuiLayerDrawEvent, Hooks, HostSide, InteractionKeyMappingTriggeredEvent, LogicalSide,
    RegisterGuiLayersEvent, Registrar, RegistrationContext, RightClickBlockEvent,
    ScreenKeyPressedPreEvent, ScreenMouseButtonPressedPreEvent, ScreenRenderPostEvent, ServerHost,
    ServerStartedEvent, ServerStoppedEvent,
};

pub mod core;
pub mod game;

#[derive(Default)]
struct Mod;

#[mod_api::horizon_mod]
impl Hooks for Mod {
    fn register(&self, r: &mut dyn Registrar, _cx: &RegistrationContext) {
        game::keys::register(r);
        game::payload::register(r);
    }

    fn register_gui_layers(
        &self,
        _cx: &RegistrationContext,
        event: &mut RegisterGuiLayersEvent<'_>,
    ) {
        game::hud::register(event);
    }

    fn add_client_reload_listeners(
        &self,
        _host: &dyn ClientHost,
        event: &mut AddClientReloadListenersEvent<'_>,
    ) {
        game::loading::add_listener(event.listeners);
    }

    fn server_started(&self, host: &mut dyn ServerHost, event: &mut ServerStartedEvent) {
        game::world::server_started(host, event.dedicated);
    }

    fn server_stopped(&self, _host: &mut dyn ServerHost, event: &mut ServerStoppedEvent) {
        if !event.dedicated {
            game::world::server_stopped();
        }
    }

    fn client_tick_post(&self, host: &mut dyn ClientHost, _event: &mut ClientTickPostEvent) {
        if game::hosting() {
            game::client_tick(host);
        }
    }

    fn client_player_logging_out(
        &self,
        host: &mut dyn ClientHost,
        _event: &mut ClientPlayerLoggingOutEvent,
    ) {
        if game::hosting() {
            game::logging_out(host);
        }
    }

    /// The attack key acts on the crosshair's target: a block that breaks at once is counted at
    /// the tick's end.
    fn interaction_key_mapping_triggered(
        &self,
        host: &mut dyn ClientHost,
        event: &mut InteractionKeyMappingTriggeredEvent,
    ) -> Flow {
        if game::hosting() {
            game::detect::attack(&*host, event.key);
        }
        Flow::Continue
    }

    /// The client's half of a right click on a block: the place check waits for the block.
    fn right_click_block(&self, host: HostSide, event: &mut RightClickBlockEvent<'_>) -> Flow {
        if let HostSide::Client(host) = host
            && event.click.side == LogicalSide::Client
            && game::hosting()
        {
            game::detect::right_click_block(&*host, event.click);
        }
        Flow::Continue
    }

    fn gui_layer_draw(&self, host: &dyn ClientHost, event: &mut GuiLayerDrawEvent<'_>) {
        if game::hosting() {
            game::hud::draw(host, event);
        }
    }

    fn screen_render_post(&self, host: &dyn ClientHost, event: &mut ScreenRenderPostEvent<'_>) {
        if game::hosting() {
            game::screen::render(host, event);
        }
    }

    fn screen_mouse_button_pressed_pre(
        &self,
        host: &dyn ClientHost,
        event: &mut ScreenMouseButtonPressedPreEvent<'_>,
    ) -> Flow {
        if !game::hosting() {
            return Flow::Continue;
        }
        game::screen::mouse_pressed(host, event)
    }

    fn screen_key_pressed_pre(
        &self,
        host: &dyn ClientHost,
        event: &mut ScreenKeyPressedPreEvent<'_>,
    ) -> Flow {
        if !game::hosting() {
            return Flow::Continue;
        }
        game::screen::key_pressed(host, event)
    }
}
