use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::spawn_local;
use yew::prelude::*;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = ["window", "__TAURI__", "core"])]
    async fn invoke(cmd: &str, args: JsValue) -> JsValue;
}

#[derive(Serialize, Deserialize, Clone)]
struct LoginCredentials {
    username: String,
    password: String,
}

#[derive(Serialize, Deserialize)]
struct LoginResult {
    success: bool,
    message: String,
}

#[derive(Serialize, Deserialize)]
struct ScanResult {
    success: bool,
    message: String,
    tracks_processed: u32,
}

#[derive(Serialize, Deserialize)]
struct ArtistScanArgs {
    artist: String,
}

#[function_component(App)]
pub fn app() -> Html {
    let is_logged_in = use_state(|| false);
    let current_user = use_state(|| None::<String>);
    let login_message = use_state(|| String::new());
    let scan_message = use_state(|| String::new());
    let is_scanning = use_state(|| false);

    // Check login status on mount
    {
        let is_logged_in = is_logged_in.clone();
        let current_user = current_user.clone();
        use_effect_with((), move |_| {
            spawn_local(async move {
                let logged_in_result = invoke("is_logged_in", JsValue::NULL).await;
                if let Ok(logged_in) = serde_wasm_bindgen::from_value::<bool>(logged_in_result) {
                    is_logged_in.set(logged_in);

                    if logged_in {
                        let user_result = invoke("get_current_user", JsValue::NULL).await;
                        if let Ok(user) =
                            serde_wasm_bindgen::from_value::<Option<String>>(user_result)
                        {
                            current_user.set(user);
                        }
                    }
                }
            });
            || {}
        });
    }

    if *is_logged_in {
        html! {
            <main class="container">
                <h1>{"Scrobble Scrubber"}</h1>

                <div class="user-info">
                    <p>{"Logged in as: "}
                        <strong>{ current_user.as_ref().unwrap_or(&"Unknown".to_string()) }</strong>
                    </p>
                </div>

                <div class="scan-section">
                    <h2>{"Scan Artist"}</h2>
                    <ArtistScanner
                        scan_message={scan_message.clone()}
                        is_scanning={is_scanning.clone()}
                    />
                </div>

                {
                    if !scan_message.is_empty() {
                        html! {
                            <div class="message">
                                <p>{ &*scan_message }</p>
                            </div>
                        }
                    } else {
                        html! {}
                    }
                }
            </main>
        }
    } else {
        html! {
            <main class="container">
                <h1>{"Scrobble Scrubber"}</h1>

                <div class="login-section">
                    <h2>{"Login to Last.fm"}</h2>
                    <LoginForm
                        is_logged_in={is_logged_in.clone()}
                        current_user={current_user.clone()}
                        login_message={login_message.clone()}
                    />
                </div>

                {
                    if !login_message.is_empty() {
                        html! {
                            <div class="message">
                                <p>{ &*login_message }</p>
                            </div>
                        }
                    } else {
                        html! {}
                    }
                }
            </main>
        }
    }
}

#[derive(Properties, PartialEq)]
struct LoginFormProps {
    is_logged_in: UseStateHandle<bool>,
    current_user: UseStateHandle<Option<String>>,
    login_message: UseStateHandle<String>,
}

#[function_component(LoginForm)]
fn login_form(props: &LoginFormProps) -> Html {
    let username_ref = use_node_ref();
    let password_ref = use_node_ref();
    let is_logging_in = use_state(|| false);

    let onsubmit = {
        let username_ref = username_ref.clone();
        let password_ref = password_ref.clone();
        let is_logged_in = props.is_logged_in.clone();
        let current_user = props.current_user.clone();
        let login_message = props.login_message.clone();
        let is_logging_in = is_logging_in.clone();

        Callback::from(move |e: SubmitEvent| {
            e.prevent_default();

            let username = username_ref
                .cast::<web_sys::HtmlInputElement>()
                .unwrap()
                .value();
            let password = password_ref
                .cast::<web_sys::HtmlInputElement>()
                .unwrap()
                .value();

            if username.is_empty() || password.is_empty() {
                login_message.set("Please enter both username and password.".to_string());
                return;
            }

            let credentials = LoginCredentials {
                username: username.clone(),
                password,
            };
            let is_logged_in = is_logged_in.clone();
            let current_user = current_user.clone();
            let login_message = login_message.clone();
            let is_logging_in = is_logging_in.clone();

            is_logging_in.set(true);
            login_message.set("Logging in...".to_string());

            spawn_local(async move {
                let args = serde_wasm_bindgen::to_value(&credentials).unwrap();
                let result = invoke("login", args).await;

                is_logging_in.set(false);

                match serde_wasm_bindgen::from_value::<LoginResult>(result) {
                    Ok(login_result) => {
                        login_message.set(login_result.message.clone());
                        if login_result.success {
                            is_logged_in.set(true);
                            current_user.set(Some(username));
                        }
                    }
                    Err(e) => {
                        login_message.set(format!("Error: {:?}", e));
                    }
                }
            });
        })
    };

    html! {
        <form onsubmit={onsubmit}>
            <div class="form-group">
                <label for="username">{"Username:"}</label>
                <input
                    ref={username_ref}
                    id="username"
                    type="text"
                    placeholder="Enter your Last.fm username"
                    disabled={*is_logging_in}
                />
            </div>

            <div class="form-group">
                <label for="password">{"Password:"}</label>
                <input
                    ref={password_ref}
                    id="password"
                    type="password"
                    placeholder="Enter your Last.fm password"
                    disabled={*is_logging_in}
                />
            </div>

            <button type="submit" disabled={*is_logging_in}>
                { if *is_logging_in { "Logging in..." } else { "Login" } }
            </button>
        </form>
    }
}

#[derive(Properties, PartialEq)]
struct ArtistScannerProps {
    scan_message: UseStateHandle<String>,
    is_scanning: UseStateHandle<bool>,
}

#[function_component(ArtistScanner)]
fn artist_scanner(props: &ArtistScannerProps) -> Html {
    let artist_ref = use_node_ref();

    let onsubmit = {
        let artist_ref = artist_ref.clone();
        let scan_message = props.scan_message.clone();
        let is_scanning = props.is_scanning.clone();

        Callback::from(move |e: SubmitEvent| {
            e.prevent_default();

            let artist = artist_ref
                .cast::<web_sys::HtmlInputElement>()
                .unwrap()
                .value();

            if artist.is_empty() {
                scan_message.set("Please enter an artist name.".to_string());
                return;
            }

            let scan_args = ArtistScanArgs {
                artist: artist.clone(),
            };
            let scan_message = scan_message.clone();
            let is_scanning = is_scanning.clone();

            is_scanning.set(true);
            scan_message.set(format!("Scanning tracks for {}...", artist));

            spawn_local(async move {
                let args = serde_wasm_bindgen::to_value(&scan_args).unwrap();
                let result = invoke("scan_artist", args).await;

                is_scanning.set(false);

                match serde_wasm_bindgen::from_value::<ScanResult>(result) {
                    Ok(scan_result) => {
                        scan_message.set(scan_result.message);
                    }
                    Err(e) => {
                        scan_message.set(format!("Error: {:?}", e));
                    }
                }
            });
        })
    };

    html! {
        <form onsubmit={onsubmit}>
            <div class="form-group">
                <label for="artist">{"Artist Name:"}</label>
                <input
                    ref={artist_ref}
                    id="artist"
                    type="text"
                    placeholder="Enter artist name to scan"
                    disabled={*props.is_scanning}
                />
            </div>

            <button type="submit" disabled={*props.is_scanning}>
                { if *props.is_scanning { "Scanning..." } else { "Scan Artist" } }
            </button>
        </form>
    }
}
