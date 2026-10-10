#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]
//! Waxbill: a desktop app for Serval. Design: `Design/` (specs, boards and tokens).

mod app;
mod browse;
mod conflicts;
mod i18n;
mod id_species;
mod output;
mod protocol;
mod results;
mod run;
mod run_bar;
mod runner;
mod screens;
mod serval;
mod settings;
mod settings_screen;
mod table;
mod theme;

use gpui_kit::component::TitleBar;
use gpui_kit::component::theme::ThemeMode;
use gpui_kit::*;

struct Args {
    design_state: Option<String>,
    serval: Option<std::path::PathBuf>,
    open: Option<std::path::PathBuf>,
    run: bool,
    size: Option<(f32, f32)>,
    mode: ThemeMode,
    english: bool,
}

fn parse_args() -> Args {
    let mut args = Args {
        design_state: None,
        serval: None,
        open: None,
        run: false,
        size: None,
        mode: ThemeMode::Light,
        english: false,
    };
    let mut iter = std::env::args().skip(1);
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--design-state" => {
                let name = iter.next().unwrap_or_default();
                if !app::DESIGN_STATES.contains(&name.as_str()) {
                    eprintln!(
                        "Unknown design state '{name}'. Known: {}",
                        app::DESIGN_STATES.join(", ")
                    );
                    std::process::exit(2);
                }
                if name.starts_with("dark-") {
                    args.mode = ThemeMode::Dark;
                }
                args.design_state = Some(name);
            }
            "--dark" => args.mode = ThemeMode::Dark,
            "--serval" => args.serval = iter.next().map(Into::into),
            "--english" => args.english = true,
            // Start the run at once (for screenshots of running and finished states).
            "--run" => args.run = true,
            // Window size for screenshots of long screens, e.g. 1200x1400.
            "--size" => {
                args.size = iter.next().and_then(|size| {
                    let (w, h) = size.split_once('x')?;
                    Some((w.parse().ok()?, h.parse().ok()?))
                })
            }
            path if !path.starts_with("--") && args.open.is_none() => args.open = Some(path.into()),
            _ => {
                eprintln!(
                    "Usage: waxbill [<folder>] [--serval <program>] [--run] [--size WxH] [--design-state <name>] [--dark] [--english]"
                );
                std::process::exit(2);
            }
        }
    }
    args
}

fn main() {
    let args = parse_args();
    let settings = settings::Settings::load();
    let appearance_given = args.mode == ThemeMode::Dark || args.design_state.is_some();
    i18n::set_language(if args.english {
        i18n::Language::English
    } else {
        settings.language
    });
    application()
        .with_assets(gpui_kit::assets::AllAssets)
        .run(move |cx| {
            gpui_kit::init(cx);
            cx.set_global(settings);
            theme::init(args.mode, cx);
            app::bind_keys(cx);
            cx.on_window_closed(|cx, _| {
                if cx.windows().is_empty() {
                    cx.quit();
                }
            })
            .detach();
            let tokens = theme::Tokens::parse(false);
            let options = WindowOptions {
                window_bounds: Some(WindowBounds::centered(
                    match args.size {
                        Some((w, h)) => size(px(w), px(h)),
                        None => size(tokens.px("window-default-w"), tokens.px("window-default-h")),
                    },
                    cx,
                )),
                window_min_size: Some(size(tokens.px("window-min-w"), tokens.px("window-min-h"))),
                app_id: Some("io.github.wsyxbcl.waxbill".into()),
                // The title bar is ours (Design/spec/shell.md §1).
                window_decorations: Some(WindowDecorations::Client),
                ..TitleBar::window_options()
            };
            let design_state = args.design_state.clone();
            let launch = app::Launch {
                serval: args.serval.clone(),
                appearance_given,
                open: args.open.clone(),
                run: args.run,
            };
            gpui_kit::open_window(options, cx, move |window, cx| {
                cx.new(|cx| app::Waxbill::new(design_state.as_deref(), launch, window, cx))
            })
            .expect("the main window opens");
            cx.activate(true);
        });
}
