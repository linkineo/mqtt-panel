//! Native menu bar with macOS conventions (app menu with About, Settings, Services, Hide, Quit) and
//! Windows/Linux conventions (Settings under Edit, Quit under File). Labels come from the UI catalogs
//! (`nativeMenu.*` keys in packages/ui/src/i18n/<lang>/ui.json), so the menu follows the app language.
use std::collections::HashMap;
use std::sync::OnceLock;
use tauri::menu::{AboutMetadataBuilder, Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::{AppHandle, Manager, Runtime};

const EN: &str = include_str!("../../../../packages/ui/src/i18n/en/ui.json");
const FR: &str = include_str!("../../../../packages/ui/src/i18n/fr/ui.json");
const DE: &str = include_str!("../../../../packages/ui/src/i18n/de/ui.json");

fn catalog(lang: &str) -> &'static HashMap<String, String> {
    static CATALOGS: OnceLock<HashMap<&'static str, HashMap<String, String>>> = OnceLock::new();
    let all = CATALOGS.get_or_init(|| {
        let parse = |s: &str| -> HashMap<String, String> {
            serde_json::from_str::<HashMap<String, String>>(s)
                .unwrap_or_default()
                .into_iter()
                .filter(|(k, _)| k.starts_with("nativeMenu."))
                .collect()
        };
        HashMap::from([("en", parse(EN)), ("fr", parse(FR)), ("de", parse(DE))])
    });
    all.get(lang).unwrap_or(&all["en"])
}

/// Label for `nativeMenu.<key>` in `lang`, falling back to English, then to the key.
pub fn label(lang: &str, key: &str) -> String {
    let k = format!("nativeMenu.{key}");
    catalog(lang).get(&k).or_else(|| catalog("en").get(&k)).cloned().unwrap_or_else(|| key.to_string())
}

fn item<R: Runtime>(app: &AppHandle<R>, id: &str, text: &str, accel: Option<&str>) -> tauri::Result<MenuItem<R>> {
    MenuItem::with_id(app, id, text, true, accel)
}

/// Copyright line of the About dialog: the website only, no company name (owner, 18 Oct 2026).
pub const ABOUT_COPYRIGHT: &str = "© 2026 mqtt-panel.com · https://mqtt-panel.com · Apache License 2.0";

pub fn build_menu<R: Runtime>(app: &AppHandle<R>, lang: &str) -> tauri::Result<Menu<R>> {
    let l = |k: &str| label(lang, k);
    let about = AboutMetadataBuilder::new()
        .name(Some("MQTT Panel"))
        .version(Some(env!("CARGO_PKG_VERSION")))
        .copyright(Some(ABOUT_COPYRIGHT))
        .build();
    let settings = item(app, "open-settings", &l("settings"), Some("CmdOrCtrl+,"))?;
    let new_conn = item(app, "new-connection", &l("newConnection"), Some("CmdOrCtrl+N"))?;
    // "Import Connections…" is not offered since change set 5 (owner, 14 Oct 2026); import returns in a later release.
    let export = item(app, "export", &l("export"), Some("CmdOrCtrl+E"))?;
    let sep = || PredefinedMenuItem::separator(app);
    let about_text = l("about");

    #[cfg(target_os = "macos")]
    let app_menu = Submenu::with_items(
        app,
        "MQTT Panel",
        true,
        &[
            &PredefinedMenuItem::about(app, Some(&about_text), Some(about.clone()))?,
            &sep()?,
            &settings,
            &sep()?,
            &PredefinedMenuItem::services(app, Some(&l("services")))?,
            &sep()?,
            &PredefinedMenuItem::hide(app, Some(&l("hide")))?,
            &PredefinedMenuItem::hide_others(app, Some(&l("hideOthers")))?,
            &PredefinedMenuItem::show_all(app, Some(&l("showAll")))?,
            &sep()?,
            &PredefinedMenuItem::quit(app, Some(&l("quit")))?,
        ],
    )?;

    let file = Submenu::with_items(
        app,
        &l("file"),
        true,
        &[&new_conn, &sep()?, &export, &sep()?, &PredefinedMenuItem::close_window(app, Some(&l("closeWindow")))?],
    )?;
    #[cfg(not(target_os = "macos"))]
    let file = {
        file.append(&sep()?)?;
        file.append(&PredefinedMenuItem::quit(app, Some(&l("quit")))?)?;
        file
    };
    let edit = Submenu::with_items(
        app,
        &l("edit"),
        true,
        &[
            &PredefinedMenuItem::undo(app, Some(&l("undo")))?,
            &PredefinedMenuItem::redo(app, Some(&l("redo")))?,
            &sep()?,
            &PredefinedMenuItem::cut(app, Some(&l("cut")))?,
            &PredefinedMenuItem::copy(app, Some(&l("copy")))?,
            &PredefinedMenuItem::paste(app, Some(&l("paste")))?,
            &PredefinedMenuItem::select_all(app, Some(&l("selectAll")))?,
            &sep()?,
            &item(app, "find", &l("find"), Some("CmdOrCtrl+F"))?,
        ],
    )?;
    #[cfg(not(target_os = "macos"))]
    {
        edit.append(&sep()?)?;
        edit.append(&settings)?;
    }
    let view = Submenu::with_items(
        app,
        &l("view"),
        true,
        &[
            &item(app, "go-connect", &l("connect"), Some("CmdOrCtrl+1"))?,
            &item(app, "go-explore", &l("explore"), Some("CmdOrCtrl+2"))?,
            &item(app, "go-publish", &l("publish"), Some("CmdOrCtrl+3"))?,
            &item(app, "go-charts", &l("charts"), Some("CmdOrCtrl+4"))?,
            &item(app, "go-broker", &l("broker"), Some("CmdOrCtrl+5"))?,
            &sep()?,
            &item(app, "command-palette", &l("commandPalette"), Some("CmdOrCtrl+K"))?,
            &sep()?,
            &PredefinedMenuItem::fullscreen(app, Some(&l("fullscreen")))?,
        ],
    )?;
    let connection = Submenu::with_items(
        app,
        &l("connection"),
        true,
        &[&item(app, "switch-connection", &l("switchConnection"), Some("CmdOrCtrl+Shift+L"))?, &sep()?, &item(app, "clear-history", &l("clearHistory"), None)?],
    )?;
    let window = Submenu::with_items(
        app,
        &l("window"),
        true,
        &[&PredefinedMenuItem::minimize(app, Some(&l("minimize")))?, &PredefinedMenuItem::maximize(app, Some(&l("zoom")))?],
    )?;
    #[cfg(target_os = "macos")]
    let help = Submenu::with_items(app, &l("help"), true, &[&item(app, "shortcuts", &l("shortcuts"), None)?])?;
    #[cfg(not(target_os = "macos"))]
    let help = Submenu::with_items(
        app,
        &l("help"),
        true,
        &[&item(app, "shortcuts", &l("shortcuts"), None)?, &PredefinedMenuItem::about(app, Some(&about_text), Some(about))?],
    )?;

    #[cfg(target_os = "macos")]
    return Menu::with_items(app, &[&app_menu, &file, &edit, &view, &connection, &window, &help]);
    #[cfg(not(target_os = "macos"))]
    Menu::with_items(app, &[&file, &edit, &view, &connection, &window, &help])
}

/// Rebuilds the menu bar in the UI's language (en, fr or de); called at start and on every language change.
#[tauri::command]
pub fn set_menu_language(app: AppHandle, lang: String) -> Result<(), String> {
    let lang = match lang.as_str() {
        "fr" | "de" => lang,
        _ => "en".to_string(),
    };
    // The assistant's texts follow the app language too (docs/MCP.md section 5).
    app.state::<crate::state::AppState>().assistant.set_lang(&lang);
    let menu = build_menu(&app, &lang).map_err(|e| e.to_string())?;
    app.set_menu(menu).map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::label;

    #[test]
    fn menu_labels_exist_in_every_language() {
        let keys = [
            "about", "settings", "services", "hide", "hideOthers", "showAll", "quit", "file", "newConnection", "export",
            "closeWindow", "edit", "undo", "redo", "cut", "copy", "paste", "selectAll", "find", "view", "connect", "explore",
            "publish", "charts", "broker", "commandPalette", "fullscreen", "connection", "switchConnection", "clearHistory",
            "window", "minimize", "zoom", "help", "shortcuts",
        ];
        for lang in ["en", "fr", "de"] {
            for k in keys {
                let v = label(lang, k);
                assert_ne!(v, k, "nativeMenu.{k} missing in {lang}");
            }
        }
        assert_eq!(label("fr", "file"), "Fichier");
        assert_eq!(label("de", "file"), "Ablage");
        assert_eq!(label("xx", "quit"), label("en", "quit"));
        // change set 5: the import item and its label are gone
        assert_eq!(label("en", "import"), "import");
    }

    #[test]
    fn about_names_the_website_only() {
        let c = super::ABOUT_COPYRIGHT;
        assert!(c.contains("mqtt-panel.com"));
        assert!(!c.to_lowercase().contains("mqtt-panel"));
        let conf = include_str!("../tauri.conf.json");
        assert!(conf.contains("\"copyright\": \"mqtt-panel.com\""));
        assert!(!conf.to_lowercase().contains("mqtt-panel labs"));
    }
}
