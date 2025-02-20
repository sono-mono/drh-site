use leptos::*;
use wasm_bindgen::prelude::*;
use js_sys::{Array, Promise};
use serde::{Serialize, Deserialize};
use log::info;
use reqwest::Client;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = navigator, js_name = geolocation)]
    fn get_current_position(success: &JsValue, error: &JsValue) -> Promise;
    #[wasm_bindgen(js_namespace = gapi)]
    fn load(api: &str, callback: &JsValue);
    #[wasm_bindgen(js_namespace = gapi.client)]
    fn init(config: &JsValue) -> Promise;
    #[wasm_bindgen(js_namespace = gapi.client)]
    fn request(config: &JsValue) -> Promise;
    #[wasm_bindgen(js_namespace = gapi.auth2)]
    fn getAuthInstance() -> JsValue;
    #[wasm_bindgen(js_name = signIn)]
    fn sign_in(this: &JsValue) -> Promise;
}

#[derive(Serialize, Deserialize, Clone)]
struct Scan {
    id: String,
    name: String,
    lat: f64,
    lon: f64,
}

#[component]
fn App(cx: Scope) -> impl IntoView {
    let (user, set_user) = create_signal(cx, Option::<String>::None);
    let (status, set_status) = create_signal(cx, String::new());
    let (scans, set_scans) = create_signal(cx, Vec::<Scan>::new());

    // GitHub Login
    let login = move |_| {
        let client_id = "your-github-client-id";
        let redirect = "https://yourusername.github.io/drh-rust/";
        let url = format!("https://github.com/login/oauth/authorize?client_id={}&redirect_uri={}&scope=user", client_id, redirect);
        web_sys::window().unwrap().location().set_href(&url).unwrap();
    };

    // Handle OAuth callback
    let params = web_sys::UrlSearchParams::new_with_str(&web_sys::window().unwrap().location().search().unwrap()).unwrap();
    if let Some(code) = params.get("code") {
        spawn_local(async move {
            let client = Client::new();
            let res = client.get(&format!("https://github-oauth-proxy.onrender.com/auth?code={}", code))
                .send().await.unwrap()
                .json::<serde_json::Value>().await.unwrap();
            if let Some(token) = res.get("access_token") {
                let user_res = client.get("https://api.github.com/user")
                    .header("Authorization", format!("token {}", token.as_str().unwrap()))
                    .send().await.unwrap()
                    .json::<serde_json::Value>().await.unwrap();
                set_user.set(Some(user_res["login"].as_str().unwrap().to_string()));
                web_sys::window().unwrap().history().unwrap().replace_state_with_url(&JsValue::NULL, "", Some("/")).unwrap();
            }
        });
    }

    // Drive Setup
    let drive_client_id = "your-drive-client-id";
    let drive_api_key = "your-drive-api-key";
    let folder_id = "your-drive-folder-id";
    let init_drive = move || {
        let config = js_sys::Object::new();
        js_sys::Reflect::set(&config, &"apiKey".into(), &drive_api_key.into()).unwrap();
        js_sys::Reflect::set(&config, &"clientId".into(), &drive_client_id.into()).unwrap();
        js_sys::Reflect::set(&config, &"scope".into(), &"https://www.googleapis.com/auth/drive.file".into()).unwrap();
        load("client:auth2", &Closure::once_into_js(move || {
            init(&config.into()).then(&Closure::once_into_js(move |_| {
                sign_in(&getAuthInstance()).then(&Closure::once_into_js(|_| {}));
            }));
        }));
    };
    init_drive();

    // Upload
    let upload = move |_| {
        let file_input = document().get_element_by_id("media").unwrap().dyn_into::<web_sys::HtmlInputElement>().unwrap();
        if let Some(file) = file_input.files().and_then(|files| files.get(0)) {
            set_status.set("Uploading...".to_string());
            let success = Closure::once_into_js(move |pos: JsValue| {
                let lat = js_sys::Reflect::get(&pos, &"coords.latitude".into()).unwrap().as_f64().unwrap();
                let lon = js_sys::Reflect::get(&pos, &"coords.longitude".into()).unwrap().as_f64().unwrap();
                let metadata = js_sys::Object::new();
                js_sys::Reflect::set(&metadata, &"name".into(), &format!("{}_{}", file.name(), chrono::Utc::now().timestamp()).into()).unwrap();
                js_sys::Reflect::set(&metadata, &"parents".into(), &Array::of1(&folder_id.into()).into()).unwrap();
                let props = js_sys::Object::new();
                js_sys::Reflect::set(&props, &"lat".into(), &lat.to_string().into()).unwrap();
                js_sys::Reflect::set(&props, &"lon".into(), &lon.to_string().into()).unwrap();
                js_sys::Reflect::set(&metadata, &"properties".into(), &props.into()).unwrap();

                let form = web_sys::FormData::new().unwrap();
                form.append_with_blob_and_filename("metadata", &Blob::new_with_str_slice(&serde_json::to_string(&metadata).unwrap()).unwrap(), "metadata.json").unwrap();
                form.append_with_blob("file", &file).unwrap();

                let config = js_sys::Object::new();
                js_sys::Reflect::set(&config, &"path".into(), &"https://www.googleapis.com/upload/drive/v3/files?uploadType=multipart".into()).unwrap();
                js_sys::Reflect::set(&config, &"method".into(), &"POST".into()).unwrap();
                js_sys::Reflect::set(&config, &"body".into(), &form.into()).unwrap();

                request(&config.into()).then(&Closure::once_into_js(move |res: JsValue| {
                    let id = js_sys::Reflect::get(&res, &"id".into()).unwrap().as_string().unwrap();
                    set_status.set(format!("Uploaded: {}", file.name()));
                    set_scans.update(|s| s.push(Scan { id, name: file.name(), lat, lon }));
                }));
            });
            let error = Closure::once_into_js(move |_| {
                set_status.set("Enable location for geo-tagging".to_string());
            });
            get_current_position(&success, &error);
        } else {
            set_status.set("Pick a file first!".to_string());
        }
    };

    // UI
    view! { cx,
        <style>"
            body { margin: 0; font-family: 'Arial', sans-serif; background: #f5f5f5; }
            .container { padding: 15px; max-width: 100%; box-sizing: border-box; }
            header { background: #1a1a1a; color: white; padding: 10px; text-align: center; font-size: 1.2em; }
            .login-btn { background: #0078d4; border: none; padding: 8px 16px; color: white; cursor: pointer; border-radius: 4px; }
            .upload-box { background: white; padding: 15px; border-radius: 8px; box-shadow: 0 2px 4px rgba(0,0,0,0.1); margin-top: 15px; }
            input[type='file'] { display: block; margin: 10px 0; width: 100%; }
            .upload-btn { background: #28a745; border: none; padding: 10px; color: white; width: 100%; border-radius: 4px; cursor: pointer; }
            .status { margin-top: 10px; font-size: 0.9em; color: #333; }
            .map-box { margin-top: 15px; height: 60vh; border-radius: 8px; overflow: hidden; }
            @media (max-width: 600px) { .container { padding: 10px; } header { font-size: 1em; } }
        "</style>
        <div class="container">
            <header>
                <Show when=move || user.get().is_none() fallback=move || view! { cx, <span>"Logged in as "{user.get().unwrap()}</span> }>
                    <button class="login-btn" on:click=login>"Login with GitHub"</button>
                </Show>
            </header>
            <div style=move || if user.get().is_some() { "display: block;" } else { "display: none;" }>
                <div class="upload-box">
                    <h2>"Upload Scan"</h2>
                    <input type="file" id="media" accept="image/*,video/*" capture="environment"/>
                    <button class="upload-btn" on:click=upload>"Upload"</button>
                    <p class="status">{status}</p>
                </div>
            </div>
            <div class="map-box">
                <h2>"Live Reality Map"</h2>
                <div id="map" style="height: 100%;"></div>
            </div>
        </div>
    }
}

#[wasm_bindgen(start)]
pub fn main() {
    console_error_panic_hook::set_once();
    console_log::init_with_level(log::Level::Info).unwrap();
    info!("Starting DRH...");
    mount_to_body(|cx| view! { cx, <App/> });
    // Map setup
    let map = js_sys::eval("L.map('map', { zoomControl: true }).setView([19.0760, 72.8777], 13);").unwrap();
    js_sys::eval("L.tileLayer('https://{s}.tile.openstreetmap.org/{z}/{x}/{y}.png', { attribution: '© OpenStreetMap' }).addTo(map);").unwrap();
    // Dynamic scan updates (simplified)
    let scans_signal = use_context::<RwSignal<Vec<Scan>>>(cx).unwrap();
    create_effect(cx, move |_| {
        let scans = scans_signal.get();
        for scan in scans {
            let js = format!(
                "L.marker([{}, {}]).addTo(map).bindPopup('<img src=\"https://drive.google.com/uc?id={}\" width=\"100\">');",
                scan.lat, scan.lon, scan.id
            );
            js_sys::eval(&js).unwrap();
        }
    });
}
