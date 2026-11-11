use sha2::{Digest, Sha256};
use std::{
    borrow::Cow,
    collections::HashMap,
    path::PathBuf,
    sync::mpsc,
    time::{Duration, Instant},
};
use tao::{
    event::{Event, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    window::WindowBuilder,
};
use twayblade_call_host::{
    Failure, Frame, MAX_FRAME, ORIGIN, QUEUE_CAPACITY, call_origin, jwt_service, read_frame,
    write_frame,
};
use wry::{NewWindowResponse, PermissionKind, PermissionResponse, WebViewBuilder, http::Response};

fn assets() -> Result<PathBuf, ()> {
    let exe = std::env::current_exe().map_err(|_| ())?;
    let parent = exe.parent().and_then(|p| p.parent()).ok_or(())?;
    let bundled = parent.join("Resources/element-call");
    let root = if bundled.is_dir() {
        bundled
    } else {
        parent.join("element-call/package/dist")
    };
    if root.join("index.html").is_file() {
        Ok(root)
    } else {
        Err(())
    }
}
fn fail(failure: Failure) -> Result<(), ()> {
    let _ = write_frame(&mut std::io::stdout().lock(), &Frame::Failed(failure));
    Err(())
}
pub fn run() -> Result<(), ()> {
    // stdin/stdout are private pipes inherited by this executable only.
    let mut stdin = std::io::stdin().lock();
    let (url, service) = loop {
        match read_frame(&mut stdin).map_err(|_| ())? {
            Frame::Configure { url, jwt_service } => break (url, jwt_service),
            Frame::Ping(n) => {
                write_frame(&mut std::io::stdout().lock(), &Frame::Pong(n)).map_err(|_| ())?
            }
            _ => return fail(Failure::InvalidConfiguration),
        }
    };
    drop(stdin);
    if !call_origin(&url) || jwt_service(&service).is_none() {
        return fail(Failure::InvalidConfiguration);
    }
    let root = match assets() {
        Ok(root) => root,
        Err(()) => return fail(Failure::AssetsMissing),
    };
    let manifest: HashMap<String, String> =
        serde_json::from_str(include_str!("../assets.manifest.json")).map_err(|_| ())?;
    let service_url = jwt_service(&service).ok_or(())?;
    let service_origin = service_url.origin().ascii_serialization();
    let ws_origin = service_origin.replacen("https://", "wss://", 1);
    let csp = format!(
        "default-src 'none'; script-src 'self' 'wasm-unsafe-eval' 'sha256-pQY0fuQAnnVQH5nQfjo80rzGkQzeN3JeAtAJ+1KcD4k='; style-src 'self' 'unsafe-inline'; img-src 'self' data: blob:; font-src 'self'; media-src 'self' blob:; worker-src 'self' blob:; connect-src 'self' {service_origin} {ws_origin}; frame-src 'none'; object-src 'none'; base-uri 'none'; form-action 'none'"
    );
    let bootstrap = include_str!("bridge.js")
        .replace(
            "__CALL_ORIGIN__",
            &serde_json::to_string(ORIGIN).map_err(|_| ())?,
        )
        .replace(
            "__JWT_SERVICE__",
            &serde_json::to_string(&service).map_err(|_| ())?,
        );
    let (tx, rx) = mpsc::sync_channel(QUEUE_CAPACITY);
    std::thread::spawn(move || {
        let mut stdin = std::io::stdin().lock();
        while let Ok(frame) = read_frame(&mut stdin) {
            if tx.send(frame).is_err() {
                return;
            }
        }
    });
    let (widget_tx, widget_rx) = mpsc::sync_channel(QUEUE_CAPACITY);
    let event_loop = EventLoop::new();
    let window = WindowBuilder::new()
        .with_title("Twayblade Call")
        .with_decorations(true)
        .with_inner_size(tao::dpi::LogicalSize::new(960.0, 720.0))
        .build(&event_loop)
        .map_err(|_| ())?;
    let webview = WebViewBuilder::new()
        .with_incognito(true)
        .with_devtools(cfg!(debug_assertions))
        .with_initialization_script(&bootstrap)
        .with_navigation_handler(|url| call_origin(&url))
        .with_new_window_req_handler(|_, _| NewWindowResponse::Deny)
        .with_download_started_handler(|_, _| false)
        // Single trusted top-level origin; frames, popups and navigation are denied.
        // Default asks WebKit/OS for media permission instead of silently granting it.
        .with_permission_handler(|kind| match kind {
            PermissionKind::Camera | PermissionKind::Microphone | PermissionKind::DisplayCapture => PermissionResponse::Default,
            _ => PermissionResponse::Deny,
        })
        .with_ipc_handler(move |request| {
            if !call_origin(&request.uri().to_string()) || request.body().len() > MAX_FRAME { std::process::exit(3); }
            let Ok(value) = serde_json::from_str::<serde_json::Value>(request.body()) else { std::process::exit(3) };
            let frame = if let Some(environment) = value.get("runtime_environment") {
                Frame::Environment {
                    secure_context: environment["secure_context"].as_bool().unwrap_or(false),
                    media_devices: environment["media_devices"].as_bool().unwrap_or(false),
                    script_transform: environment["script_transform"].as_bool().unwrap_or(false),
                }
            } else if let Some(health) = value.get("runtime_service_health") {
                Frame::ServiceHealth(health.as_bool().unwrap_or(false))
            } else if let Some(failure) = value.get("runtime_failure") {
                Frame::Failed(if failure == "PermissionDenied" { Failure::PermissionDenied } else { Failure::BridgeRejected })
            } else { Frame::Widget(value) };
            if widget_tx.try_send(frame).is_err() { std::process::exit(3); }
        })
        .with_custom_protocol("twayblade-call".into(), move |_, request| {
            let path = request.uri().path().trim_start_matches('/');
            let path = if path.is_empty() { "index.html" } else { path };
            let bytes = manifest.get(path).and_then(|expected| {
                let bytes = std::fs::read(root.join(path)).ok()?;
                (format!("{:x}", Sha256::digest(&bytes)) == *expected).then_some(bytes)
            });
            let mime = match path.rsplit('.').next().unwrap_or("") {
                "html" => "text/html", "js" => "text/javascript", "css" => "text/css", "json" => "application/json", "wasm" => "application/wasm", "svg" => "image/svg+xml", "png" => "image/png", "woff2" => "font/woff2", _ => "application/octet-stream",
            };
            let status = if bytes.is_some() { 200 } else { 404 };
            Response::builder().status(status)
                .header("Content-Type", mime)
                .header("Content-Security-Policy", &csp)
                .header("X-Content-Type-Options", "nosniff")
                // WK custom schemes have the opaque DOM origin "null".
                .header("Access-Control-Allow-Origin", "null")
                .header("Permissions-Policy", "camera=(self), microphone=(self), display-capture=(self), geolocation=(), payment=(), usb=(), serial=()")
                .body(Cow::Owned(bytes.unwrap_or_default())).expect("static headers")
        })
        .with_url(&url).build(&window).map_err(|_| ())?;
    write_frame(&mut std::io::stdout().lock(), &Frame::Ready).map_err(|_| ())?;
    event_loop.run(move |event, _, control| {
        *control = ControlFlow::WaitUntil(Instant::now() + Duration::from_millis(20));
        if let Event::WindowEvent {
            event: WindowEvent::CloseRequested,
            ..
        } = event
        {
            *control = ControlFlow::Exit;
        }
        for _ in 0..QUEUE_CAPACITY {
            match rx.try_recv() {
                Ok(Frame::Ping(n)) => {
                    if write_frame(&mut std::io::stdout().lock(), &Frame::Pong(n)).is_err() {
                        *control = ControlFlow::Exit;
                    }
                }
                Ok(Frame::Widget(value)) => {
                    // serde JSON is an expression; no string interpolation of raw widget text.
                    let script = format!("window.__twaybladeReceive({value})");
                    if webview.evaluate_script(&script).is_err() {
                        *control = ControlFlow::Exit;
                    }
                }
                Ok(Frame::Shutdown) | Err(mpsc::TryRecvError::Disconnected) => {
                    *control = ControlFlow::Exit;
                    break;
                }
                Err(mpsc::TryRecvError::Empty) => break,
                _ => {
                    *control = ControlFlow::Exit;
                    break;
                }
            }
        }
        for frame in widget_rx.try_iter().take(QUEUE_CAPACITY) {
            if write_frame(&mut std::io::stdout().lock(), &frame).is_err() {
                *control = ControlFlow::Exit;
            }
        }
    });
}
