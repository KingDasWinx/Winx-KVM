//! Entry-point da aplicação Tauri do Winx-KVM.
//!
//! Aqui montamos:
//! - O subscriber de tracing (logs estruturados, filtro por `WINX_LOG`)
//! - O event bus (`winx-application`)
//! - Os adapters concretos de infra (TOML store, keyring)
//! - O `AppState` e estados por bounded context
//! - Os Tauri commands e o forwarder de eventos para o frontend

mod app_state;
mod commands;
mod events;
mod telemetry;

use std::sync::{Arc, RwLock};

use tauri::{
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    Manager,
};
use tokio::sync::Mutex;
use tracing::{error, info};
use winx_application::{
    ports::{IdentityStore, MonitorBackend, SecretStore, WINX_KVM_PORT},
    ClipboardService, DiscoveryService, EnsureDevice, InputControlService, PairingService,
    TransportService,
};
use winx_domain::{discovery::DiscoveryRegistry, shared::ids::PeerId};
use winx_infra::{
    generate_or_load_quic_cert, network_config, network_watcher::NetworkWatcher,
    ArboardClipboardBackend, KeyringSecretStore, MdnsDiscoveryAdapter, QuicTransportAdapter,
    TomlConfigStore, TomlIdentityStore, Win32InputBackend, Win32MonitorBackend,
};

use app_state::{
    AppState, ClipboardState, DiscoveryState, IdentityState, InputControlState, PairingState,
    TransportState,
};

/// Toda inicialização async (identity, QUIC, etc.) acontece aqui, dentro do runtime Tokio,
/// antes do event loop da webview ser iniciado pelo Tauri.
struct InitializedServices {
    ensure_device: Arc<EnsureDevice>,
    identity_store: Arc<dyn IdentityStore>,
    discovery: Arc<DiscoveryService>,
    pairing: Arc<PairingService>,
    transport: Arc<TransportService>,
    input_control: Arc<InputControlService>,
    clipboard: Arc<ClipboardService>,
    config_store: Arc<TomlConfigStore>,
    device: winx_domain::identity::Device,
}

fn init_services(
    rt: &tokio::runtime::Runtime,
    bus: winx_application::bus::EventBus,
    data_dir: std::path::PathBuf,
) -> Result<InitializedServices, Box<dyn std::error::Error>> {
    let identity_store = Arc::new(TomlIdentityStore::new(data_dir.clone()));
    let secret_store: Arc<dyn SecretStore> = Arc::new(KeyringSecretStore::new());
    let ensure_device = Arc::new(EnsureDevice::new(
        Arc::clone(&identity_store) as _,
        Arc::clone(&secret_store),
        bus.clone(),
    ));

    let username = std::env::var("COMPUTERNAME").unwrap_or_else(|_| "My PC".to_string());
    rt.block_on(ensure_device.execute(&username))
        .map_err(|e| std::io::Error::other(e.to_string()))?;

    let signing_key = rt
        .block_on(secret_store.load_signing_key())
        .map_err(|e| std::io::Error::other(e.to_string()))?
        .ok_or("signing key deve existir após ensure_device")?;

    let cert_path = data_dir.join("quic_cert.pem");
    let key_path = data_dir.join("quic_key.pem");
    let (cert_der, key_der) = generate_or_load_quic_cert(&signing_key, &cert_path, &key_path)
        .map_err(|e| std::io::Error::other(e.to_string()))?;

    let trusted_keys: Vec<[u8; 32]> = rt
        .block_on(identity_store.load_peers())
        .map_err(|e| std::io::Error::other(e.to_string()))?
        .iter()
        .map(|p| *p.public_key.as_bytes())
        .collect();
    let trusted_keys = Arc::new(RwLock::new(trusted_keys));

    let config_store = Arc::new(TomlConfigStore::new(data_dir.clone()));
    let app_config = config_store
        .load_or_create()
        .map_err(|e| std::io::Error::other(e.to_string()))?;

    let mdns = Arc::new(
        MdnsDiscoveryAdapter::new(&app_config.discovery.interfaces)
            .map_err(|e| std::io::Error::other(e.to_string()))?,
    );
    let registry = Arc::new(Mutex::new(DiscoveryRegistry::new()));
    let discovery = Arc::new(DiscoveryService::new(
        mdns,
        Arc::clone(&registry),
        bus.clone(),
    ));

    let pairing = Arc::new(PairingService::new(
        Arc::clone(&identity_store) as _,
        bus.clone(),
    ));

    // quinn::Endpoint::server requer runtime Tokio ativo na thread — block_on garante isso.
    let quic_adapter = Arc::new(
        rt.block_on(async { QuicTransportAdapter::new(cert_der, key_der, trusted_keys) })
            .map_err(|e| std::io::Error::other(e.to_string()))?,
    );
    let transport = Arc::new(TransportService::new(
        quic_adapter,
        Arc::clone(&identity_store) as _,
        Arc::clone(&registry),
        bus.clone(),
    ));

    let input_backend = Arc::new(Win32InputBackend::new());
    let monitor_backend: Arc<dyn MonitorBackend> = Arc::new(Win32MonitorBackend);
    let input_control = Arc::new(InputControlService::new(
        input_backend,
        monitor_backend,
        Arc::clone(&transport),
        bus.clone(),
    ));

    let device = rt
        .block_on(identity_store.load_device())
        .map_err(|e| std::io::Error::other(e.to_string()))?
        .ok_or("device deve existir após ensure_device")?;
    let local_peer_id = PeerId::from_uuid(device.id.as_uuid());

    let clipboard_backend = Arc::new(ArboardClipboardBackend::new());
    let clipboard = Arc::new(ClipboardService::new(
        clipboard_backend,
        Arc::clone(&transport),
        bus.clone(),
        local_peer_id,
        app_config.clipboard.auto_sync,
    ));

    Ok(InitializedServices {
        ensure_device,
        identity_store: Arc::clone(&identity_store) as Arc<dyn IdentityStore>,
        discovery,
        pairing,
        transport,
        input_control,
        clipboard,
        config_store,
        device,
    })
}

#[allow(clippy::too_many_lines)]
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    telemetry::init();
    info!(version = env!("CARGO_PKG_VERSION"), "iniciando Winx-KVM");

    // Sub-comando helper executado pelo filho elevado via UAC.
    if std::env::args().any(|a| a == "--setup-firewall") {
        if !network_config::is_elevated() {
            eprintln!("--setup-firewall requires admin privileges");
            std::process::exit(2);
        }
        let exe = match std::env::current_exe() {
            Ok(e) => e,
            Err(e) => {
                eprintln!("current_exe failed: {}", e);
                std::process::exit(1);
            }
        };
        match network_config::reconfigure(&exe) {
            Ok(_) => {
                info!("firewall reconfigured successfully");
                std::process::exit(0);
            }
            Err(e) => {
                eprintln!("reconfigure failed: {}", e);
                std::process::exit(1);
            }
        }
    }

    // 1. Verificar e configurar firewall
    let firewall_ok = match network_config::inspect() {
        Ok(status) => {
            if status.needs_fix() {
                tracing::warn!(issues = ?status.issues(), "firewall precisa de ajuste");
                match network_config::request_firewall_setup_via_uac() {
                    Ok(0) => {
                        info!("firewall reconfigurado com sucesso");
                        true
                    }
                    Ok(code) => {
                        tracing::warn!(exit_code = code, "helper saiu com erro");
                        false
                    }
                    Err(e) => {
                        tracing::warn!(?e, "UAC recusado ou falhou");
                        false
                    }
                }
            } else {
                info!("firewall já está OK");
                true
            }
        }
        Err(e) => {
            tracing::warn!(?e, "inspect falhou");
            false
        }
    };

    // Runtime Tokio criado antes do Tauri. O setup hook roda dentro do event loop
    // da webview (sem contexto async), então toda a inicialização async acontece aqui.
    let rt = tokio::runtime::Runtime::new().expect("falha ao criar runtime Tokio");

    // Registra o handle no Tauri para que tauri::async_runtime::spawn funcione
    // nos commands e nas tarefas de background lançadas depois do setup.
    tauri::async_runtime::set(rt.handle().clone());

    // 2. Iniciar NetworkWatcher
    let (_watcher, net_events_rx) = match NetworkWatcher::start() {
        Ok((w, rx)) => (Some(w), Some(rx)),
        Err(e) => {
            tracing::warn!(?e, "NetworkWatcher indisponível");
            (None, None)
        }
    };
    let _watcher = _watcher; // Explicitly keep watcher alive

    let app_state = AppState::new();
    let bus = app_state.bus.clone();

    // Determina o data_dir antes do Builder — mesmo caminho que o Tauri usaria no Windows:
    // %APPDATA%\<bundle-identifier>
    let data_dir = {
        let appdata = std::env::var("APPDATA").expect("variável APPDATA não encontrada");
        std::path::PathBuf::from(appdata).join("br.com.winxkvm.app")
    };
    std::fs::create_dir_all(&data_dir).expect("falha ao criar data_dir");

    let services = init_services(&rt, bus, data_dir).expect("falha na inicialização dos serviços");

    let device_for_discovery = services.device.clone();

    // Spawna as tarefas de background no runtime já registrado.
    let discovery_bg = Arc::clone(&services.discovery);
    let device_clone = device_for_discovery.clone();
    rt.spawn(async move {
        if let Err(err) = discovery_bg.start_for_device(&device_clone).await {
            error!(?err, "falha ao iniciar discovery no startup");
        }
    });

    let transport_bg = Arc::clone(&services.transport);
    rt.spawn(async move {
        if let Err(err) = transport_bg.start(WINX_KVM_PORT).await {
            error!(?err, "falha ao iniciar transport QUIC");
        }
    });

    let pairing_bg = Arc::clone(&services.pairing);
    rt.spawn(async move {
        pairing_bg.spawn_cleanup_task();
    });

    // Spawnar network watcher task se disponível
    if let Some(net_rx) = net_events_rx {
        let discovery_bg = Arc::clone(&services.discovery);
        rt.spawn(async move {
            let mut rx = net_rx;
            while let Some(_ev) = rx.recv().await {
                if let Some(peer_id) = discovery_bg.get_own_peer_id().await {
                    let info = winx_application::ports::discovery::AnnounceInfo {
                        peer_id,
                        username: "".to_string(),
                        fingerprint: "".to_string(),
                        port: WINX_KVM_PORT,
                    };
                    if let Err(e) = discovery_bg.reannounce(&info).await {
                        tracing::debug!(?e, "reannounce falhou");
                    } else {
                        info!("mDNS reannounced após mudança de rede");
                    }
                }
            }
        });
    }

    tauri::Builder::default()
        .manage(app_state)
        .setup(move |app| {
            app.manage(IdentityState {
                ensure_device: services.ensure_device,
                identity_store: services.identity_store,
            });
            app.manage(DiscoveryState {
                discovery: services.discovery,
            });
            app.manage(PairingState {
                pairing: services.pairing,
            });
            app.manage(TransportState {
                transport: services.transport,
            });
            app.manage(InputControlState {
                input_control: services.input_control,
            });
            app.manage(ClipboardState {
                clipboard: services.clipboard,
            });
            app.manage(services.config_store);
            app.manage(app_state::FirewallState {
                is_configured: Arc::new(Mutex::new(firewall_ok)),
            });

            events::install_forwarder(app.handle().clone());

            let show_item = MenuItem::with_id(app, "show", "Show", true, None::<&str>)?;
            let separator = PredefinedMenuItem::separator(app)?;
            let quit_item = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
            let tray_menu = Menu::with_items(app, &[&show_item, &separator, &quit_item])?;

            let _tray = TrayIconBuilder::new()
                .icon(
                    app.default_window_icon()
                        .ok_or("ícone padrão da janela não encontrado")?
                        .clone(),
                )
                .menu(&tray_menu)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "show" => {
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.set_focus();
                        }
                    }
                    "quit" => {
                        app.exit(0);
                    }
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        let app = tray.app_handle();
                        if let Some(window) = app.get_webview_window("main") {
                            if window.is_visible().unwrap_or(true) {
                                let _ = window.hide();
                            } else {
                                let _ = window.show();
                                let _ = window.set_focus();
                            }
                        }
                    }
                })
                .build(app.handle())?;

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::app_info,
            commands::ping,
            commands::get_device_info,
            commands::update_device_username,
            commands::list_discovered_peers,
            commands::initiate_pairing,
            commands::accept_pairing,
            commands::submit_pin,
            commands::cancel_pairing,
            commands::open_connection,
            commands::disconnect_peer,
            commands::get_connection_stats,
            commands::get_focus_state,
            commands::enable_input_control,
            commands::get_clipboard_auto_sync,
            commands::set_clipboard_auto_sync,
            commands::enable_clipboard_sync,
            commands::get_firewall_status,
            commands::reconfigure_firewall,
            commands::export_diagnostics,
        ])
        .run(tauri::generate_context!())
        .expect("erro ao executar a aplicação Tauri");

    // Mantém o runtime vivo até o app fechar (run() é bloqueante).
    drop(rt);
}
