#![recursion_limit = "512"]

#[cfg(target_os = "macos")]
mod contribute;
mod detect;
mod i18n;
#[cfg(target_os = "macos")]
mod models;
mod report;
#[cfg(target_os = "macos")]
mod review;
mod review_state;
mod sources;

use std::ffi::OsString;
use std::fs::OpenOptions;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

fn main() {
    i18n::set_lang(i18n::system());
    if let Err(error) = run() {
        eprintln!("{}: {error}", i18n::pick("骂了吗", "Malema"));
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args_os().skip(1);
    let mut home = None;
    let mut html = None;
    let mut audit = None;
    let mut summary = false;
    let mut help = false;
    while let Some(arg) = args.next() {
        let value = |value: Option<OsString>| -> io::Result<PathBuf> {
            value.map(PathBuf::from).ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidInput,
                    i18n::pick("选项缺少路径", "the option needs a path"),
                )
            })
        };
        match arg.to_str() {
            Some("--home") => home = Some(value(args.next())?),
            Some("--report") => html = Some(value(args.next())?),
            Some("--audit") => audit = Some(value(args.next())?),
            Some("--summary") => summary = true,
            Some("--help" | "-h") => help = true,
            _ => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!(
                        "{} {}",
                        i18n::pick("无法识别的参数", "unrecognized argument"),
                        arg.to_string_lossy()
                    ),
                )
                .into());
            }
        }
    }
    let use_env = home.is_none();
    let home = home
        .or_else(|| std::env::var_os("HOME").map(PathBuf::from))
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                i18n::pick("找不到用户目录", "cannot find the home directory"),
            )
        })?;
    if !home.is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            i18n::pick(
                "用户目录不可读或不存在",
                "the home directory is unreadable or missing",
            ),
        )
        .into());
    }
    i18n::set_lang(i18n::load(&home));
    if help {
        println!("{}", helptext());
        return Ok(());
    }
    if html.is_none() && audit.is_none() && !summary {
        return desktop(home, use_env);
    }
    if html.is_some() && html == audit {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            i18n::pick(
                "报表和复核数据需要不同的输出文件",
                "the report and the audit data need different output files",
            ),
        )
        .into());
    }
    let report = report::build(&home, use_env);
    if let Some(path) = html {
        private_write(&path, report.render(false).as_bytes())?;
    }
    if let Some(path) = audit {
        private_write(&path, &serde_json::to_vec_pretty(&report)?)?;
    }
    println!("{}", serde_json::to_string_pretty(&report.summary())?);
    Ok(())
}

fn helptext() -> String {
    format!(
        "{}\n  malema                         {}\n  malema --report FILE.html      {}\n  malema --summary               {}\n  malema --audit FILE.json       {}\n  --home DIR                     {}",
        i18n::pick("骂了吗", "Malema"),
        i18n::pick("打开桌面应用", "open the desktop app"),
        i18n::pick("扫描并导出私人报表", "scan and export a private report"),
        i18n::pick("扫描并输出计数", "scan and print the counts"),
        i18n::pick(
            "扫描并导出带片段的本地复核数据",
            "scan and export local audit data with excerpts"
        ),
        i18n::pick(
            "使用指定目录作为会话根目录",
            "use this directory as the session root"
        ),
    )
}

fn private_write(path: &Path, content: &[u8]) -> io::Result<()> {
    let name = path.file_name().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            i18n::pick("需要文件名", "a file name is required"),
        )
    })?;
    let temporary = path.with_file_name(format!(
        ".{}.{}.tmp",
        name.to_string_lossy(),
        std::process::id()
    ));
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(&temporary)?;
    let result = (|| {
        file.write_all(content)?;
        file.sync_all()?;
        std::fs::rename(&temporary, path)
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    result
}

#[cfg(target_os = "macos")]
fn install_app_menu(
    window: &tao::window::Window,
    updater: Option<&objc2::rc::Retained<objc2::runtime::AnyObject>>,
) -> Result<(), &'static str> {
    use objc2::{MainThreadMarker, MainThreadOnly, sel};
    use objc2_app_kit::{NSApplication, NSEventModifierFlags, NSMenu, NSMenuItem, NSWindow};
    use objc2_foundation::NSString;
    use tao::platform::macos::WindowExtMacOS;

    let mtm = MainThreadMarker::new().ok_or(i18n::pick(
        "应用菜单必须在主线程创建",
        "the app menu must be built on the main thread",
    ))?;
    let app = NSApplication::sharedApplication(mtm);
    let menu = NSMenu::new(mtm);
    let app_item = NSMenuItem::new(mtm);
    let app_menu = NSMenu::initWithTitle(
        NSMenu::alloc(mtm),
        &NSString::from_str(i18n::pick("骂了吗", "Malema")),
    );
    // Tao owns this NSWindow for the lifetime of the app's event loop.
    let native_window = unsafe { window.ns_window().cast::<NSWindow>().as_ref() }
        .ok_or(i18n::pick("找不到应用窗口", "the app window is missing"))?;
    let quit = unsafe {
        NSMenuItem::initWithTitle_action_keyEquivalent(
            NSMenuItem::alloc(mtm),
            &NSString::from_str(i18n::pick("退出骂了吗", "Quit Malema")),
            Some(sel!(performClose:)),
            &NSString::from_str("q"),
        )
    };
    // Route through CloseRequested so an active review can cancel before exit.
    unsafe { quit.setTarget(Some(native_window.as_ref())) };
    quit.setKeyEquivalentModifierMask(NSEventModifierFlags::Command);
    if let Some(controller) = updater {
        let item = unsafe {
            NSMenuItem::initWithTitle_action_keyEquivalent(
                NSMenuItem::alloc(mtm),
                &NSString::from_str(i18n::pick("检查更新…", "Check for Updates…")),
                Some(sel!(checkForUpdates:)),
                &NSString::from_str(""),
            )
        };
        unsafe { item.setTarget(Some(controller)) };
        app_menu.addItem(&item);
        app_menu.addItem(&NSMenuItem::separatorItem(mtm));
    }
    let about = unsafe {
        NSMenuItem::initWithTitle_action_keyEquivalent(
            NSMenuItem::alloc(mtm),
            &NSString::from_str(i18n::pick("关于骂了吗", "About Malema")),
            Some(sel!(orderFrontStandardAboutPanel:)),
            &NSString::from_str(""),
        )
    };
    app_menu.addItem(&about);
    app_menu.addItem(&NSMenuItem::separatorItem(mtm));
    app_menu.addItem(&quit);
    app_item.setSubmenu(Some(&app_menu));
    menu.addItem(&app_item);

    let edit_item = NSMenuItem::new(mtm);
    let edit_menu = NSMenu::initWithTitle(
        NSMenu::alloc(mtm),
        &NSString::from_str(i18n::pick("编辑", "Edit")),
    );
    // The focused WebKit editor receives AppKit's standard responder actions.
    for (title, action, key) in [
        (i18n::pick("剪切", "Cut"), sel!(cut:), "x"),
        (i18n::pick("复制", "Copy"), sel!(copy:), "c"),
        (i18n::pick("粘贴", "Paste"), sel!(paste:), "v"),
        (i18n::pick("全选", "Select All"), sel!(selectAll:), "a"),
    ] {
        let item = unsafe {
            NSMenuItem::initWithTitle_action_keyEquivalent(
                NSMenuItem::alloc(mtm),
                &NSString::from_str(title),
                Some(action),
                &NSString::from_str(key),
            )
        };
        item.setKeyEquivalentModifierMask(NSEventModifierFlags::Command);
        edit_menu.addItem(&item);
    }
    edit_item.setSubmenu(Some(&edit_menu));
    menu.addItem(&edit_item);
    app.setMainMenu(Some(&menu));
    Ok(())
}

#[cfg(target_os = "macos")]
fn sparkle_updater() -> Option<objc2::rc::Retained<objc2::runtime::AnyObject>> {
    use objc2::msg_send;
    use objc2::rc::Allocated;
    use objc2::runtime::{AnyClass, AnyObject, Bool};
    use objc2_foundation::{NSBundle, NSString};

    let url = NSBundle::mainBundle()
        .privateFrameworksURL()?
        .URLByAppendingPathComponent(&NSString::from_str("Sparkle.framework"))?;
    let bundle = NSBundle::bundleWithURL(&url)?;
    // The updater is embedded and signed as part of the app bundle.
    if !unsafe { bundle.load() } {
        eprintln!(
            "{}",
            i18n::pick(
                "无法载入 Sparkle 更新组件",
                "the Sparkle updater component failed to load"
            )
        );
        return None;
    }
    let class = AnyClass::get(c"SPUStandardUpdaterController")?;
    let allocated: Allocated<AnyObject> = unsafe { msg_send![class, alloc] };
    unsafe {
        msg_send![
            allocated,
            initWithStartingUpdater: Bool::YES,
            updaterDelegate: None::<&AnyObject>,
            userDriverDelegate: None::<&AnyObject>,
        ]
    }
}

#[cfg(target_os = "macos")]
fn set_dock_icon() {
    use objc2::{AnyThread, MainThreadMarker};
    use objc2_app_kit::{NSApplication, NSImage};
    use objc2_foundation::{NSBundle, NSString};

    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    let Some(icon_url) = NSBundle::mainBundle().URLForResource_withExtension(
        Some(&NSString::from_str("AppIcon")),
        Some(&NSString::from_str("icns")),
    ) else {
        return;
    };
    if let Some(icon) = NSImage::initWithContentsOfURL(NSImage::alloc(), &icon_url) {
        // The packaged image also keeps the running tile independent of the system icon cache.
        unsafe { NSApplication::sharedApplication(mtm).setApplicationIconImage(Some(&icon)) };
    }
}

#[cfg(target_os = "macos")]
fn desktop(home: PathBuf, use_env: bool) -> Result<(), Box<dyn std::error::Error>> {
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };
    use tao::dpi::{LogicalPosition, LogicalSize};
    use tao::event::{Event, StartCause, WindowEvent};
    use tao::event_loop::{ControlFlow, EventLoopBuilder};
    use tao::window::WindowBuilder;
    use wry::WebViewBuilder;

    const REPOSITORY: &str = "https://github.com/ay27/malema";

    enum Action {
        ReportReady,
        Scan,
        ScanCached(serde_json::Value),
        ScanDone(
            Option<(serde_json::Value, String)>,
            Option<contribute::Collection>,
        ),
        Contribution(contribute::Request),
        ContributionDone(contribute::SyncResult),
        Review(review::Request),
        CancelReview,
        ReviewProgress(usize, usize),
        ReviewDone(review::Outcome),
        OpenRepository,
        Language(i18n::Lang),
    }

    let event_loop = EventLoopBuilder::<Action>::with_user_event().build();
    let proxy = event_loop.create_proxy();
    let window = WindowBuilder::new()
        .with_title(i18n::pick("骂了吗", "Malema"))
        .with_inner_size(LogicalSize::new(1130.0, 790.0))
        .with_min_inner_size(LogicalSize::new(390.0, 410.0))
        .with_position(LogicalPosition::new(105.0, 65.0))
        .build(&event_loop)?;
    let updater = sparkle_updater();
    install_app_menu(&window, updater.as_ref())?;
    let ipc = proxy.clone();
    let webview = WebViewBuilder::new()
        .with_incognito(true)
        .with_navigation_handler(|url| url == "about:blank")
        .with_ipc_handler(move |message| {
            let action = match message.body().as_str() {
                "report-ready" => Some(Action::ReportReady),
                "rescan" => Some(Action::Scan),
                "cancel-review" => Some(Action::CancelReview),
                "open-repository" => Some(Action::OpenRepository),
                "language:zh" => Some(Action::Language(i18n::Lang::Zh)),
                "language:en" => Some(Action::Language(i18n::Lang::En)),
                raw if raw.starts_with("contribution:") && raw.len() < 512 => {
                    serde_json::from_str(raw.trim_start_matches("contribution:"))
                        .ok()
                        .map(Action::Contribution)
                }
                raw if raw.len() < 32768 => serde_json::from_str::<review::Request>(raw)
                    .ok()
                    .map(Action::Review),
                _ => None,
            };
            if let Some(action) = action {
                let _ = ipc.send_event(action);
            }
        })
        .with_html(report::desktop_shell())
        .build(&window)?;
    let mut report_ready = false;
    let mut busy = false;
    let mut rescan_pending = false;
    let mut review_cancel: Option<Arc<AtomicBool>> = None;
    let mut closing = false;
    let mut contribution = contribute::Controller::load(&home);
    let mut latest: Option<contribute::Collection> = None;
    let mut sent: Option<Arc<AtomicBool>> = None;
    let mut sync_pending = false;
    let scan_proxy = proxy.clone();
    let scan_home = home.clone();
    let cache = home.join("Library/Caches/Malema").join(format!(
        "report-v1-{}-{}.json",
        detect::RULESET,
        if use_env { "default" } else { "custom" }
    ));
    let launch_scan = move |show: bool, read_cache: bool, since: Option<i64>| {
        let proxy = scan_proxy.clone();
        let home = scan_home.clone();
        let cache = cache.clone();
        std::thread::spawn(move || {
            let review_stamp = serde_json::to_value(review_state::modified_at(&home))
                .unwrap_or(serde_json::Value::Null);
            if read_cache
                && let Ok(bytes) = std::fs::read(&cache)
                && let Ok(data) = serde_json::from_slice::<serde_json::Value>(&bytes)
                && data["rows"].is_array()
                && data["clients"].is_array()
                && data["review_stamp"] == review_stamp
            {
                let _ = proxy.send_event(Action::ScanCached(data));
            }
            let mut counts = contribute::Collection::new(contribute::now(), since);
            let report = report::build_observed(&home, use_env, |source, turn, hit| {
                counts.collect(source, turn, hit);
            });
            let payload = show.then(|| {
                let mut data = report.data();
                data["review_stamp"] = review_stamp;
                // Keep only the statistics on disk. The records are kept in the open window.
                if let Some(dir) = cache.parent()
                    && std::fs::create_dir_all(dir).is_ok()
                    && let Ok(bytes) = serde_json::to_vec(&data)
                {
                    let _ = private_write(&cache, &bytes);
                }
                (data, report.records_html())
            });
            let _ = proxy.send_event(Action::ScanDone(payload, Some(counts)));
        });
    };
    event_loop.run(move |event, _, flow| {
        *flow = ControlFlow::Wait;
        let mut update = false;
        let mut try_sync = false;
        match event {
            Event::NewEvents(StartCause::Init) => set_dock_icon(),
            Event::UserEvent(Action::ReportReady) if !report_ready => {
                report_ready = true;
                busy = true;
                launch_scan(true, true, contribution.since());
            }
            Event::UserEvent(Action::Scan) => {
                if busy {
                    rescan_pending = true;
                } else {
                    busy = true;
                    launch_scan(true, false, contribution.since());
                }
            }
            Event::UserEvent(Action::ScanCached(data)) => {
                let _ = webview.evaluate_script(&format!("window.scanCached({data})"));
            }
            Event::UserEvent(Action::ScanDone(payload, counts)) => {
                busy = false;
                if let Some(counts) = counts {
                    latest = Some(counts);
                }
                let requested = std::mem::take(&mut rescan_pending);
                if requested {
                    busy = true;
                    launch_scan(true, false, contribution.since());
                } else if let Some((data, records)) = payload {
                    if let Ok(records) = serde_json::to_string(&records) {
                        let _ =
                            webview.evaluate_script(&format!("window.scanDone({data},{records})"));
                    }
                }
                try_sync = true;
                update = true;
            }
            Event::UserEvent(Action::Contribution(request)) => {
                use contribute::Request;
                update = true;
                match request {
                    Request::Enable if contribution.since().is_none() => {
                        if let Some(counts) = latest.as_mut() {
                            let since = contribute::now();
                            if contribution.enable(since).is_ok() {
                                counts.start(since);
                                try_sync = true;
                            } else {
                                contribution.message = i18n::pick(
                                    "无法保存贡献设置。",
                                    "could not save the contribution setting.",
                                )
                                .to_owned();
                            }
                        }
                    }
                    Request::Disable if contribution.since().is_some() => {
                        if contribution.disable().is_ok() {
                            if let Some(cancel) = &sent {
                                cancel.store(true, Ordering::Relaxed);
                            }
                        } else {
                            contribution.message = i18n::pick(
                                "无法保存关闭设置。请检查应用配置目录。",
                                "could not save the off switch; check the application support directory.",
                            )
                            .to_owned();
                        }
                    }
                    Request::Open => {
                        let _ = std::process::Command::new("/usr/bin/open")
                            .arg(contribute::LEADERBOARD_URL)
                            .spawn();
                    }
                    _ => {}
                }
            }
            Event::UserEvent(Action::ContributionDone(result)) => {
                sent = None;
                if contribution.finish(result).is_err() {
                    contribution.message = i18n::pick(
                    "无法保存同步回执。下次会安全重试。",
                    "could not save the sync receipt; the next run will retry safely.",
                )
                .to_owned();
                }
                if closing && review_cancel.is_none() {
                    *flow = ControlFlow::Exit;
                }
                update = true;
            }
            Event::UserEvent(Action::Review(request)) if review_cancel.is_none() => {
                let cancel = Arc::new(AtomicBool::new(false));
                review_cancel = Some(cancel.clone());
                let proxy = proxy.clone();
                let home = home.clone();
                std::thread::spawn(move || {
                    let outcome = review::run(&home, use_env, request, &cancel, |done, total| {
                        let _ = proxy.send_event(Action::ReviewProgress(done, total));
                    });
                    let _ = proxy.send_event(Action::ReviewDone(outcome));
                });
            }
            Event::UserEvent(Action::CancelReview) => {
                if let Some(cancel) = &review_cancel {
                    cancel.store(true, Ordering::Relaxed);
                }
            }
            Event::UserEvent(Action::ReviewProgress(done, total)) => {
                let _ =
                    webview.evaluate_script(&format!("window.reviewProgress?.({done},{total})"));
            }
            Event::UserEvent(Action::ReviewDone(outcome)) => {
                review_cancel = None;
                if closing {
                    if sent.is_none() {
                        *flow = ControlFlow::Exit;
                    }
                } else {
                    if outcome.saved {
                        let _ = webview.evaluate_script("window.scanStarted?.()");
                        if busy {
                            rescan_pending = true;
                        } else {
                            busy = true;
                            launch_scan(true, false, contribution.since());
                        }
                    }
                    if let Ok(json) = serde_json::to_string(&outcome) {
                        let _ = webview.evaluate_script(&format!("window.reviewDone?.({json})"));
                    }
                }
            }
            Event::UserEvent(Action::OpenRepository) => {
                let _ = std::process::Command::new("/usr/bin/open")
                    .arg(REPOSITORY)
                    .spawn();
            }
            Event::UserEvent(Action::Language(next)) if next != i18n::lang() => {
                let _ = i18n::save(&home, next);
                i18n::set_lang(next);
                let _ = install_app_menu(&window, updater.as_ref());
                let _ = webview.load_html(&report::desktop_shell());
            }
            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                ..
            } => {
                if let Some(cancel) = &review_cancel {
                    closing = true;
                    cancel.store(true, Ordering::Relaxed);
                    window.set_visible(false);
                } else if sent.is_some() {
                    closing = true;
                    window.set_visible(false);
                } else {
                    *flow = ControlFlow::Exit;
                }
            }
            _ => {}
        }
        sync_pending |= try_sync;
        if sync_pending
            && sent.is_none()
            && !busy
            && !closing
            && let Some(counts) = latest
                .as_ref()
                .filter(|counts| counts.since() == contribution.since())
        {
            sync_pending = false;
            match contribution.prepare(&counts.live, contribute::now()) {
                Ok(batches) if !batches.is_empty() => {
                    let cancel = Arc::new(AtomicBool::new(false));
                    sent = Some(cancel.clone());
                    let proxy = proxy.clone();
                    std::thread::spawn(move || {
                        let result = contribute::sync(batches, &cancel);
                        let _ = proxy.send_event(Action::ContributionDone(result));
                    });
                }
                Err(_) => {
                    contribution.message = i18n::pick(
                        "无法保存待上报计数。",
                        "could not save the pending counts.",
                    )
                    .to_owned();
                }
                _ => {}
            }
        }
        if update
            && let Ok(status) =
                serde_json::to_string(&contribution.status(latest.is_some(), sent.is_some()))
        {
            let _ = webview.evaluate_script(&format!("window.contributionStatus?.({status})"));
        }
        let _keep_window_alive = &window;
        let _keep_updater_alive = &updater;
    });
}

#[cfg(not(target_os = "macos"))]
fn desktop(_: PathBuf, _: bool) -> Result<(), Box<dyn std::error::Error>> {
    Err("桌面窗口目前支持 macOS。可用 --report FILE.html 导出报表。".into())
}
