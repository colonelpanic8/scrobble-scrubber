use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::spawn_local;
use yew::prelude::*;

use crate::rule_workshop::RuleWorkshop;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = ["window", "__TAURI__", "core"])]
    pub async fn invoke(cmd: &str, args: JsValue) -> JsValue;

    #[wasm_bindgen(js_namespace = ["window", "__TAURI__", "event"])]
    pub async fn listen(event: &str, handler: &js_sys::Function) -> JsValue;
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

#[derive(Serialize, Deserialize, Clone, PartialEq)]
pub struct RewriteRule {
    pub id: String,
    pub name: String,
    pub pattern: String,
    pub replacement: String,
    pub enabled: bool,
}

#[derive(Serialize, Deserialize)]
struct TestRuleRequest {
    pattern: String,
    replacement: String,
    test_input: String,
}

#[derive(Serialize, Deserialize)]
struct TestRuleResult {
    success: bool,
    result: String,
    error: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, PartialEq)]
struct TrackProcessingUpdate {
    track_name: String,
    artist_name: String,
    original_track: String,
    original_artist: String,
    rules_applied: Vec<String>,
    status: String,
}

#[derive(Serialize, Deserialize, Clone, PartialEq)]
pub struct TrackInfo {
    pub name: String,
    pub artist: String,
    pub album: Option<String>,
    pub playcount: u32,
    pub timestamp: Option<u64>,
}

#[derive(Serialize, Deserialize)]
pub struct FetchTracksRequest {
    pub artist: Option<String>,
    pub limit: Option<u32>,
}

#[derive(Serialize, Deserialize)]
pub struct TestRulesRequest {
    pub rules: Vec<RewriteRule>,
    pub tracks: Vec<TrackInfo>,
}

#[derive(Serialize, Deserialize)]
pub struct TestRulesResult {
    pub track_results: Vec<TrackTestResult>,
}

#[derive(Serialize, Deserialize, Clone, PartialEq)]
pub struct TrackTestResult {
    pub original_track: TrackInfo,
    pub would_change: bool,
    pub new_track_name: Option<String>,
    pub new_artist_name: Option<String>,
    pub rules_applied: Vec<String>,
}

#[function_component(App)]
pub fn app() -> Html {
    let is_logged_in = use_state(|| false);
    let current_user = use_state(|| None::<String>);
    let login_message = use_state(String::new);
    let scan_message = use_state(String::new);
    let is_scanning = use_state(|| false);
    let rewrite_rules = use_state(Vec::<RewriteRule>::new);
    let processing_updates = use_state(Vec::<TrackProcessingUpdate>::new);
    let active_tab = use_state(|| "scan".to_string()); // "scan", "rules", "processing", "workshop"

    // Check login status and load data on mount
    {
        let is_logged_in = is_logged_in.clone();
        let current_user = current_user.clone();
        let rewrite_rules = rewrite_rules.clone();
        use_effect_with((), move |_| {
            spawn_local(async move {
                // Check login status
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

                // Load rewrite rules
                let rules_result = invoke("get_rewrite_rules", JsValue::NULL).await;
                if let Ok(rules) = serde_wasm_bindgen::from_value::<Vec<RewriteRule>>(rules_result)
                {
                    rewrite_rules.set(rules);
                }

                // Subscribe to processing updates
                let _ = invoke("subscribe_to_processing_updates", JsValue::NULL).await;
            });
            || {}
        });
    }

    // Listen for processing updates
    {
        let processing_updates = processing_updates.clone();
        use_effect_with((), move |_| {
            spawn_local(async move {
                let callback =
                    wasm_bindgen::closure::Closure::wrap(Box::new(move |event: JsValue| {
                        if let Ok(payload) = js_sys::Reflect::get(&event, &"payload".into()) {
                            if let Ok(update) =
                                serde_wasm_bindgen::from_value::<TrackProcessingUpdate>(payload)
                            {
                                let mut updates = (*processing_updates).clone();

                                // If it's a "processing" status, add it. If "completed", update the existing one
                                if update.status == "processing" {
                                    updates.push(update);
                                } else if update.status == "completed" {
                                    // Find and update the corresponding processing entry
                                    for existing_update in &mut updates {
                                        if existing_update.track_name == update.track_name
                                            && existing_update.artist_name == update.artist_name
                                            && existing_update.status == "processing"
                                        {
                                            existing_update.status = "completed".to_string();
                                            break;
                                        }
                                    }
                                }

                                processing_updates.set(updates);
                            }
                        }
                    })
                        as Box<dyn Fn(JsValue)>);

                let _ = listen("processing-update", callback.as_ref().unchecked_ref()).await;
                callback.forget(); // Keep the callback alive
            });
            || {}
        });
    }

    if *is_logged_in {
        let tab_click = {
            let active_tab = active_tab.clone();
            Callback::from(move |tab: String| {
                active_tab.set(tab);
            })
        };

        html! {
            <main class="container">
                <h1>{"Scrobble Scrubber"}</h1>

                <div class="user-info">
                    <p>{"Logged in as: "}
                        <strong>{ current_user.as_ref().unwrap_or(&"Unknown".to_string()) }</strong>
                    </p>
                </div>

                // Tab navigation
                <div class="tabs">
                    <button
                        class={if *active_tab == "scan" { "tab active" } else { "tab" }}
                        onclick={let tab_click = tab_click.clone(); Callback::from(move |_| tab_click.emit("scan".to_string()))}
                    >
                        {"Scan Artists"}
                    </button>
                    <button
                        class={if *active_tab == "rules" { "tab active" } else { "tab" }}
                        onclick={let tab_click = tab_click.clone(); Callback::from(move |_| tab_click.emit("rules".to_string()))}
                    >
                        {"Rewrite Rules ({}"} { rewrite_rules.iter().filter(|r| r.enabled).count() } {")"}
                    </button>
                    <button
                        class={if *active_tab == "processing" { "tab active" } else { "tab" }}
                        onclick={let tab_click = tab_click.clone(); Callback::from(move |_| tab_click.emit("processing".to_string()))}
                    >
                        {"Processing Log"}
                    </button>
                    <button
                        class={if *active_tab == "workshop" { "tab active" } else { "tab" }}
                        onclick={Callback::from(move |_| tab_click.emit("workshop".to_string()))}
                    >
                        {"Workshop"}
                    </button>
                </div>

                // Tab content
                {
                    match active_tab.as_str() {
                        "scan" => html! {
                            <div class="tab-content">
                                <h2>{"Scan Artist"}</h2>
                                <ArtistScanner
                                    scan_message={scan_message.clone()}
                                    is_scanning={is_scanning.clone()}
                                />
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
                            </div>
                        },
                        "rules" => html! {
                            <div class="tab-content">
                                <RewriteRulesManager rules={rewrite_rules.clone()} />
                            </div>
                        },
                        "processing" => html! {
                            <div class="tab-content">
                                <ProcessingLog updates={processing_updates.clone()} />
                            </div>
                        },
                        "workshop" => html! {
                            <div class="tab-content">
                                <RuleWorkshop rules={rewrite_rules.clone()} />
                            </div>
                        },
                        _ => html! {}
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
                // Create a JS object with named parameter
                let args = js_sys::Object::new();
                js_sys::Reflect::set(
                    &args,
                    &"credentials".into(),
                    &serde_wasm_bindgen::to_value(&credentials).unwrap(),
                )
                .unwrap();
                let args_value = wasm_bindgen::JsValue::from(args);

                let result = invoke("login", args_value).await;

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
                        login_message.set(format!("Error: {e:?}"));
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
            scan_message.set(format!("Scanning tracks for {artist}..."));

            spawn_local(async move {
                let args = serde_wasm_bindgen::to_value(&scan_args).unwrap();
                let result = invoke("scan_artist", args).await;

                is_scanning.set(false);

                match serde_wasm_bindgen::from_value::<ScanResult>(result) {
                    Ok(scan_result) => {
                        scan_message.set(scan_result.message);
                    }
                    Err(e) => {
                        scan_message.set(format!("Error: {e:?}"));
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

#[derive(Properties, PartialEq)]
struct RewriteRulesManagerProps {
    rules: UseStateHandle<Vec<RewriteRule>>,
}

#[function_component(RewriteRulesManager)]
fn rewrite_rules_manager(props: &RewriteRulesManagerProps) -> Html {
    let show_editor = use_state(|| false);
    let editing_rule = use_state(|| None::<RewriteRule>);

    let toggle_editor = {
        let show_editor = show_editor.clone();
        let editing_rule = editing_rule.clone();
        Callback::from(move |rule: Option<RewriteRule>| {
            editing_rule.set(rule);
            show_editor.set(!*show_editor);
        })
    };

    html! {
        <div class="rules-manager">
            <div class="rules-header">
                <h2>{"Rewrite Rules"}</h2>
                <button onclick={let toggle_editor = toggle_editor.clone(); Callback::from(move |_| toggle_editor.emit(None))}>
                    {"Add New Rule"}
                </button>
            </div>

            <div class="rules-list">
                {
                    props.rules.iter().map(|rule| {
                        let rule_clone = rule.clone();
                        let toggle_editor = toggle_editor.clone();
                        html! {
                            <div key={rule.id.clone()} class={format!("rule-item {}", if rule.enabled { "enabled" } else { "disabled" })}>
                                <div class="rule-content">
                                    <h3>{&rule.name}</h3>
                                    <div class="rule-pattern">
                                        <strong>{"Pattern: "}</strong>
                                        <code>{&rule.pattern}</code>
                                    </div>
                                    <div class="rule-replacement">
                                        <strong>{"Replacement: "}</strong>
                                        <code>{if rule.replacement.is_empty() { "(empty)" } else { &rule.replacement }}</code>
                                    </div>
                                </div>
                                <div class="rule-actions">
                                    <button
                                        onclick={let rule_clone = rule_clone.clone(); let toggle_editor = toggle_editor.clone();
                                                 Callback::from(move |_| toggle_editor.emit(Some(rule_clone.clone())))}
                                    >
                                        {"Edit"}
                                    </button>
                                    <button class="status-btn">
                                        {if rule.enabled { "Enabled" } else { "Disabled" }}
                                    </button>
                                </div>
                            </div>
                        }
                    }).collect::<Html>()
                }
            </div>

            {
                if *show_editor {
                    html! {
                        <RuleEditor
                            rule={(*editing_rule).clone()}
                            on_close={toggle_editor.clone()}
                        />
                    }
                } else {
                    html! {}
                }
            }
        </div>
    }
}

#[derive(Properties, PartialEq)]
struct RuleEditorProps {
    rule: Option<RewriteRule>,
    on_close: Callback<Option<RewriteRule>>,
}

#[function_component(RuleEditor)]
fn rule_editor(props: &RuleEditorProps) -> Html {
    let name_ref = use_node_ref();
    let pattern_ref = use_node_ref();
    let replacement_ref = use_node_ref();
    let test_input_ref = use_node_ref();

    let test_result = use_state(|| None::<TestRuleResult>);

    let test_rule = {
        let pattern_ref = pattern_ref.clone();
        let replacement_ref = replacement_ref.clone();
        let test_input_ref = test_input_ref.clone();
        let test_result = test_result.clone();

        Callback::from(move |_| {
            let pattern = pattern_ref
                .cast::<web_sys::HtmlInputElement>()
                .unwrap()
                .value();
            let replacement = replacement_ref
                .cast::<web_sys::HtmlInputElement>()
                .unwrap()
                .value();
            let test_input = test_input_ref
                .cast::<web_sys::HtmlTextAreaElement>()
                .unwrap()
                .value();

            let test_result = test_result.clone();
            spawn_local(async move {
                let request = TestRuleRequest {
                    pattern,
                    replacement,
                    test_input,
                };

                let args = js_sys::Object::new();
                js_sys::Reflect::set(
                    &args,
                    &"request".into(),
                    &serde_wasm_bindgen::to_value(&request).unwrap(),
                )
                .unwrap();
                let args_value = wasm_bindgen::JsValue::from(args);

                let result = invoke("test_rule", args_value).await;
                if let Ok(test_result_data) =
                    serde_wasm_bindgen::from_value::<TestRuleResult>(result)
                {
                    test_result.set(Some(test_result_data));
                }
            });
        })
    };

    html! {
        <div class="rule-editor-overlay">
            <div class="rule-editor">
                <div class="editor-header">
                    <h3>{if props.rule.is_some() { "Edit Rule" } else { "New Rule" }}</h3>
                    <button onclick={let on_close = props.on_close.clone(); Callback::from(move |_| on_close.emit(None))}>
                        {"✕"}
                    </button>
                </div>

                <div class="editor-content">
                    <div class="form-group">
                        <label>{"Rule Name:"}</label>
                        <input
                            ref={name_ref}
                            type="text"
                            value={props.rule.as_ref().map(|r| r.name.clone()).unwrap_or_default()}
                            placeholder="Descriptive name for this rule"
                        />
                    </div>

                    <div class="form-group">
                        <label>{"Pattern (Regex):"}</label>
                        <input
                            ref={pattern_ref}
                            type="text"
                            value={props.rule.as_ref().map(|r| r.pattern.clone()).unwrap_or_default()}
                            placeholder="Regular expression pattern"
                        />
                    </div>

                    <div class="form-group">
                        <label>{"Replacement:"}</label>
                        <input
                            ref={replacement_ref}
                            type="text"
                            value={props.rule.as_ref().map(|r| r.replacement.clone()).unwrap_or_default()}
                            placeholder="Replacement text (leave empty to remove)"
                        />
                    </div>

                    <div class="test-section">
                        <h4>{"Test Rule"}</h4>
                        <div class="form-group">
                            <label>{"Test Input:"}</label>
                            <textarea
                                ref={test_input_ref}
                                placeholder="Enter text to test the rule against"
                                rows="3"
                            />
                        </div>
                        <button onclick={test_rule} type="button">{"Test Rule"}</button>

                        {
                            if let Some(result) = test_result.as_ref() {
                                html! {
                                    <div class={format!("test-result {}", if result.success { "success" } else { "error" })}>
                                        <strong>{"Result: "}</strong>
                                        <div class="result-text">{&result.result}</div>
                                        {
                                            if let Some(error) = &result.error {
                                                html! { <div class="error-text">{error}</div> }
                                            } else {
                                                html! {}
                                            }
                                        }
                                    </div>
                                }
                            } else {
                                html! {}
                            }
                        }
                    </div>

                    <div class="editor-actions">
                        <button type="button">{"Save Rule"}</button>
                        <button
                            type="button"
                            onclick={let on_close = props.on_close.clone(); Callback::from(move |_| on_close.emit(None))}
                        >
                            {"Cancel"}
                        </button>
                    </div>
                </div>
            </div>
        </div>
    }
}

#[derive(Properties, PartialEq)]
struct ProcessingLogProps {
    updates: UseStateHandle<Vec<TrackProcessingUpdate>>,
}

#[function_component(ProcessingLog)]
fn processing_log(props: &ProcessingLogProps) -> Html {
    html! {
        <div class="processing-log">
            <h2>{"Track Processing Log"}</h2>

            {
                if props.updates.is_empty() {
                    html! {
                        <div class="empty-log">
                            <p>{"No tracks processed yet. Start a scan to see processing updates here."}</p>
                        </div>
                    }
                } else {
                    html! {
                        <div class="log-entries">
                            {
                                props.updates.iter().map(|update| {
                                    html! {
                                        <div key={format!("{}-{}", update.artist_name, update.track_name)}
                                             class={format!("log-entry {}", update.status)}>
                                            <div class="track-info">
                                                <strong>{format!("{} - {}", update.artist_name, update.track_name)}</strong>
                                                {
                                                    if update.original_track != update.track_name || update.original_artist != update.artist_name {
                                                        html! {
                                                            <div class="original">
                                                                {"(originally: "}{&update.original_artist}{" - "}{&update.original_track}{")"}
                                                            </div>
                                                        }
                                                    } else {
                                                        html! {}
                                                    }
                                                }
                                            </div>
                                            <div class="rules-applied">
                                                {
                                                    if !update.rules_applied.is_empty() {
                                                        html! {
                                                            <>
                                                                <strong>{"Rules applied: "}</strong>
                                                                {update.rules_applied.join(", ")}
                                                            </>
                                                        }
                                                    } else {
                                                        html! { <span class="no-rules">{"No rules applied"}</span> }
                                                    }
                                                }
                                            </div>
                                        </div>
                                    }
                                }).collect::<Html>()
                            }
                        </div>
                    }
                }
            }
        </div>
    }
}
