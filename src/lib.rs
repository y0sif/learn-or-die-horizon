//! Learn or Die (Horizon port): the game-independent teacher (`core`) wired to the game's hooks
//! (`game`). Ported from Learn or Die by Laren, All Rights Reserved, with permission.
//!
//! The teacher lives on the client, as in the original: detection, his voice, the HUD and the
//! question screen all run there, so he works on any server. The integrated server's half hands
//! the client the world seed for the progress file's world key, applies an answer's
//! consequences to the host's own player, and tells the client what only it knows of that player:
//! the vehicle, the fishing hook, the fire, the sheep's colours and the kills (`game::server`).
//!
//! The client hooks act only for this window's own game: a split-screen player's game, in this
//! process too, would otherwise drive the same teacher with its own player (`game::hosting`).
use mod_api::{
    AddClientReloadListenersEvent, BlockScreenEffect, ClientAttackTargetEvent, ClientHost,
    ClientPlayerLoggingOutEvent, ClientTickPostEvent, EntityInteractEvent,
    ExtractBlockScreenEffectEvent, Flow, GuiDraw, GuiLayerDrawEvent, Hand, HealthHudEvent, Hooks,
    HostSide, InputKeyEvent, InteractionKeyMappingTriggeredEvent, InteractionResult,
    KeyMappingPreEvent, LivingDeathEvent, LogicalSide, PlayerTickPostEvent, RegisterGuiLayersEvent,
    Registrar, RegistrationContext, RenderLivingPreEvent, RightClickBlockEvent,
    ScreenCharTypedPreEvent, ScreenKeyPressedPreEvent, ScreenMouseButtonPressedPreEvent,
    ScreenMouseButtonReleasedPreEvent, ScreenMouseDraggedPreEvent, ScreenRenderPostEvent,
    ServerHost, ServerStartedEvent, ServerStoppedEvent,
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
        if !event.dedicated {
            game::server::reset();
        }
    }

    fn server_stopped(&self, _host: &mut dyn ServerHost, event: &mut ServerStoppedEvent) {
        if !event.dedicated {
            game::world::server_stopped();
            game::server::reset();
        }
    }

    /// The integrated server ticked a player: for its host's own, a waiting drop is thrown and
    /// the client is told what it cannot read of them (`game::detect::HostFacts`).
    fn player_tick_post(&self, host: HostSide, event: &mut PlayerTickPostEvent) {
        if let HostSide::Server(host) = host
            && let Some(facts) = game::server::host_tick(host, event.player)
        {
            game::detect::set_host_facts(Some(facts));
        }
    }

    /// A living death on the server: one the host's own player dealt the killing blow to, by hand
    /// or with a projectile of theirs, is said as their kill.
    fn living_death(&self, host: &mut dyn ServerHost, event: &mut LivingDeathEvent<'_>) -> Flow {
        if !event.cancelled
            && let Some((entity, kind)) =
                game::server::kill_credit(host, event.entity, event.kind, event.source.causing)
        {
            game::detect::host_killed(entity, kind);
        }
        Flow::Continue
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

    /// A key bound to a gameplay mapping goes down or up while playing: the drop key is kept for
    /// the tick (the `mapping` read does not answer it).
    fn key_mapping_pre(
        &self,
        _host: &mut dyn ClientHost,
        event: &mut KeyMappingPreEvent<'_>,
    ) -> Flow {
        if game::hosting() {
            game::detect::key_mapping(event.mapping, event.pressed);
        }
        Flow::Continue
    }

    /// Every key event, the window's and the control socket's: the socket's drop key, which
    /// `key_mapping_pre` is not told.
    fn input_key(&self, _host: &mut dyn ClientHost, event: &mut InputKeyEvent<'_>) {
        if game::hosting() {
            game::detect::input_key(event.key, event.action);
        }
    }

    /// A frame draws the fire over the first-person view: the player burns (a client reads no
    /// fire ticks).
    fn extract_block_screen_effect(
        &self,
        _host: &dyn ClientHost,
        event: &mut ExtractBlockScreenEffectEvent,
    ) -> Flow {
        if event.effect == BlockScreenEffect::Fire && game::hosting() {
            game::detect::fire_drawn();
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

    /// The attack key hits the crosshair's entity (the first target): `hit.<entity>`, and a
    /// living one is watched for the kill.
    fn client_attack_target(
        &self,
        host: &mut dyn ClientHost,
        event: &mut ClientAttackTargetEvent<'_>,
    ) -> Flow {
        if let Some(&target) = event.targets.first()
            && game::hosting()
        {
            game::detect::attacked(&*host, target);
        }
        Flow::Continue
    }

    /// The client's half of a right click on an entity, main hand: `feed.<entity>`. On the
    /// server, a sheep's colour is read again, as a dye may change it.
    fn entity_interact(
        &self,
        host: HostSide,
        event: &mut EntityInteractEvent<'_>,
    ) -> Option<InteractionResult> {
        match host {
            HostSide::Client(host) if event.hand == Hand::Main && game::hosting() => {
                game::detect::entity_used(&*host, event.entity, event.kind, event.stack);
            }
            HostSide::Server(_) => game::server::entity_used(event.entity),
            _ => {}
        }
        None
    }

    /// Every living entity drawn: one dying is kept for the tick (a client reads no health of
    /// another entity). Gated on `hosting` inside, after the comparison that lets almost every
    /// entity through untouched.
    fn render_living_pre(
        &self,
        _host: &dyn ClientHost,
        event: &mut RenderLivingPreEvent<'_>,
    ) -> Flow {
        game::detect::drawn(event.entity);
        Flow::Continue
    }

    /// The health bar's inputs tell whether the world is a Hardcore one. Horizon draws its own
    /// bar.
    fn health_hud(
        &self,
        _host: &mut dyn ClientHost,
        event: &mut HealthHudEvent<'_>,
    ) -> Option<Vec<GuiDraw>> {
        if game::hosting() {
            game::detect::set_hardcore(event.inputs.hardcore);
        }
        None
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

    fn screen_char_typed_pre(
        &self,
        host: &dyn ClientHost,
        event: &mut ScreenCharTypedPreEvent<'_>,
    ) -> Flow {
        if !game::hosting() {
            return Flow::Continue;
        }
        game::screen::char_typed(host, event)
    }

    fn screen_mouse_button_released_pre(
        &self,
        host: &dyn ClientHost,
        event: &mut ScreenMouseButtonReleasedPreEvent<'_>,
    ) -> Flow {
        if !game::hosting() {
            return Flow::Continue;
        }
        game::screen::mouse_released(host, event)
    }

    fn screen_mouse_dragged_pre(
        &self,
        host: &dyn ClientHost,
        event: &mut ScreenMouseDraggedPreEvent<'_>,
    ) -> Flow {
        if !game::hosting() {
            return Flow::Continue;
        }
        game::screen::mouse_dragged(host, event)
    }
}
