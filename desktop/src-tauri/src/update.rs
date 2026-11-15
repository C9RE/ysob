//! App updates. The endpoint and the public key are baked in at build time from YSOB_UPDATE_URL
//! and YSOB_UPDATE_PUBKEY, so a build without them (one built from source) just says it doesn't
//! update itself. Only updates signed with the matching private key are ever installed, and the
//! endpoint must be HTTPS. The UI reaches these commands, the folder picker and the link opener,
//! and nothing else of Tauri's (capabilities/app.json).

use serde_json::{json, Value};
use std::sync::Mutex;
use tauri::{AppHandle, Manager};
use tauri_plugin_updater::UpdaterExt;

/// Download progress the UI polls: bytes so far, total if known, and the phase.
#[derive(Default)]
pub struct Progress(pub Mutex<(u64, Option<u64>, &'static str)>);

/// Why the last install failed, kept across the engine restart that follows it, so the page
/// that comes back can say so. Read once.
#[derive(Default)]
pub struct Failed(pub Mutex<Option<String>>);

pub fn pubkey() -> Option<&'static str> {
    option_env!("YSOB_UPDATE_PUBKEY").map(str::trim).filter(|k| !k.is_empty())
}

fn endpoint() -> Option<&'static str> {
    option_env!("YSOB_UPDATE_URL").map(str::trim).filter(|u| !u.is_empty())
}

fn updater(app: &AppHandle) -> Result<tauri_plugin_updater::Updater, String> {
    let off = || "This build doesn't check for updates.".to_string();
    let key = pubkey().ok_or_else(off)?;
    let url = tauri::Url::parse(endpoint().ok_or_else(off)?).map_err(|e| e.to_string())?;
    // The updater refuses plain HTTP in release builds too; this says why in our own words.
    if url.scheme() != "https" {
        return Err("The update address isn't HTTPS, so it isn't used.".into());
    }
    app.updater_builder().pubkey(key).endpoints(vec![url]).map_err(|e| e.to_string())?.build().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn app_version(app: AppHandle) -> Value {
    let failed = app.state::<Failed>().0.lock().unwrap().take();
    json!({ "version": app.package_info().version.to_string(), "updates": pubkey().is_some() && endpoint().is_some(), "failed": failed })
}

/// Is there a newer version? Null when up to date.
#[tauri::command]
pub async fn update_check(app: AppHandle) -> Result<Value, String> {
    let up = updater(&app)?;
    match up.check().await.map_err(|e| e.to_string())? {
        Some(u) => Ok(json!({ "version": u.version, "current": u.current_version, "notes": u.body, "date": u.date.map(|d| d.to_string()) })),
        None => Ok(Value::Null),
    }
}

#[tauri::command]
pub fn update_progress(app: AppHandle) -> Value {
    let p = app.state::<Progress>();
    let g = p.0.lock().unwrap();
    json!({ "done": g.0, "total": g.1, "phase": g.2 })
}

/// Download the update, stop the engine (Windows locks a running program's files), install,
/// and start again on the new version. On Windows the installer takes over and reopens the app
/// itself; elsewhere the app restarts here. If the install fails the engine is started again,
/// so the app is never left without one.
#[tauri::command]
pub async fn update_install(app: AppHandle) -> Result<(), String> {
    let up = updater(&app)?;
    let u = up.check().await.map_err(|e| e.to_string())?.ok_or("Already up to date.")?;
    *app.state::<Progress>().0.lock().unwrap() = (0, None, "downloading");
    let a = app.clone();
    let bytes = u
        .download(
            move |chunk, total| {
                let p = a.state::<Progress>();
                let mut g = p.0.lock().unwrap();
                g.0 += chunk as u64;
                g.1 = total;
            },
            || {},
        )
        .await
        .map_err(|e| {
            *app.state::<Progress>().0.lock().unwrap() = (0, None, "");
            e.to_string()
        })?;
    app.state::<Progress>().0.lock().unwrap().2 = "installing";
    crate::stop(&app);
    if let Err(e) = u.install(bytes) {
        let why = e.to_string();
        *app.state::<Progress>().0.lock().unwrap() = (0, None, "");
        *app.state::<Failed>().0.lock().unwrap() = Some(why.clone());
        let again = app.clone();
        std::thread::spawn(move || crate::boot(again));
        return Err(why);
    }
    app.restart();
}
