use wasm_bindgen_futures::spawn_local;
use yew::prelude::*;

use crate::app::{
    invoke, FetchTracksRequest, RewriteRule, TestRulesRequest, TestRulesResult, TrackInfo,
};

#[derive(Properties, PartialEq)]
pub struct RuleWorkshopProps {
    pub rules: UseStateHandle<Vec<RewriteRule>>,
}

#[function_component(RuleWorkshop)]
pub fn rule_workshop(props: &RuleWorkshopProps) -> Html {
    let tracks = use_state(Vec::<TrackInfo>::new);
    let test_results = use_state(|| None::<TestRulesResult>);
    let is_loading = use_state(|| false);
    let artist_filter = use_state(String::new);
    let limit = use_state(|| 100u32);

    let fetch_tracks = {
        let tracks = tracks.clone();
        let is_loading = is_loading.clone();
        let artist_filter = artist_filter.clone();
        let limit = limit.clone();

        Callback::from(move |_| {
            let tracks = tracks.clone();
            let is_loading = is_loading.clone();
            let artist_filter = artist_filter.clone();
            let limit = limit.clone();

            is_loading.set(true);

            spawn_local(async move {
                let request = FetchTracksRequest {
                    artist: if artist_filter.is_empty() {
                        None
                    } else {
                        Some((*artist_filter).clone())
                    },
                    limit: Some(*limit),
                };

                let args = js_sys::Object::new();
                js_sys::Reflect::set(
                    &args,
                    &"request".into(),
                    &serde_wasm_bindgen::to_value(&request).unwrap(),
                )
                .unwrap();
                let args_value = wasm_bindgen::JsValue::from(args);

                let result = invoke("fetch_tracks", args_value).await;
                is_loading.set(false);

                match serde_wasm_bindgen::from_value::<Vec<TrackInfo>>(result) {
                    Ok(fetched_tracks) => {
                        tracks.set(fetched_tracks);
                    }
                    Err(e) => {
                        web_sys::console::error_1(&format!("Failed to fetch tracks: {e:?}").into());
                    }
                }
            });
        })
    };

    let test_rules = {
        let rules = props.rules.clone();
        let tracks = tracks.clone();
        let test_results = test_results.clone();

        Callback::from(move |_| {
            let rules = rules.clone();
            let tracks = tracks.clone();
            let test_results = test_results.clone();

            spawn_local(async move {
                let request = TestRulesRequest {
                    rules: (*rules).clone(),
                    tracks: (*tracks).clone(),
                };

                let args = js_sys::Object::new();
                js_sys::Reflect::set(
                    &args,
                    &"request".into(),
                    &serde_wasm_bindgen::to_value(&request).unwrap(),
                )
                .unwrap();
                let args_value = wasm_bindgen::JsValue::from(args);

                let result = invoke("test_rules_on_tracks", args_value).await;

                match serde_wasm_bindgen::from_value::<TestRulesResult>(result) {
                    Ok(results) => {
                        test_results.set(Some(results));
                    }
                    Err(e) => {
                        web_sys::console::error_1(&format!("Failed to test rules: {e:?}").into());
                    }
                }
            });
        })
    };

    let on_artist_change = {
        let artist_filter = artist_filter.clone();
        Callback::from(move |e: Event| {
            let target = e.target_unchecked_into::<web_sys::HtmlInputElement>();
            artist_filter.set(target.value());
        })
    };

    let on_limit_change = {
        let limit = limit.clone();
        Callback::from(move |e: Event| {
            let target = e.target_unchecked_into::<web_sys::HtmlInputElement>();
            if let Ok(new_limit) = target.value().parse::<u32>() {
                limit.set(new_limit);
            }
        })
    };

    html! {
        <div class="rule-workshop">
            <h2>{"Rule Workshop"}</h2>
            <p>{"Test how your rewrite rules would apply to recent tracks or tracks from a specific artist."}</p>

            <div class="workshop-controls">
                <div class="form-group inline">
                    <label>{"Artist (optional):"}</label>
                    <input
                        type="text"
                        value={(*artist_filter).clone()}
                        onchange={on_artist_change}
                        placeholder="Leave empty for recent tracks"
                    />
                </div>

                <div class="form-group inline">
                    <label>{"Limit:"}</label>
                    <input
                        type="number"
                        value={limit.to_string()}
                        onchange={on_limit_change}
                        min="1"
                        max="500"
                    />
                </div>

                <button onclick={fetch_tracks} disabled={*is_loading}>
                    { if *is_loading { "Loading..." } else { "Load Tracks" } }
                </button>

                <button onclick={test_rules} disabled={tracks.is_empty()}>
                    {"Test Rules"}
                </button>
            </div>

            {
                if !tracks.is_empty() {
                    html! {
                        <div class="tracks-info">
                            <h3>{format!("Loaded {} tracks", tracks.len())}</h3>
                        </div>
                    }
                } else {
                    html! {}
                }
            }

            {
                if let Some(results) = test_results.as_ref() {
                    let affected_count = results.track_results.iter().filter(|r| r.would_change).count();
                    html! {
                        <div class="test-results">
                            <h3>{format!("Rule Test Results: {} of {} tracks would change", affected_count, results.track_results.len())}</h3>

                            <div class="results-list">
                                {
                                    results.track_results.iter().map(|result| {
                                        let track_class = if result.would_change { "track-result changed" } else { "track-result" };
                                        html! {
                                            <div key={format!("{}-{}", result.original_track.artist, result.original_track.name)} class={track_class}>
                                                <div class="track-info">
                                                    <div class="original">
                                                        <strong>{format!("{} - {}", result.original_track.artist, result.original_track.name)}</strong>
                                                        {
                                                            if let Some(album) = &result.original_track.album {
                                                                html! { <span class="album">{format!(" from {}", album)}</span> }
                                                            } else {
                                                                html! {}
                                                            }
                                                        }
                                                    </div>

                                                    {
                                                        if result.would_change {
                                                            html! {
                                                                <div class="changed">
                                                                    <span class="arrow">{"→"}</span>
                                                                    <strong>{
                                                                        format!("{} - {}",
                                                                            result.original_track.artist,
                                                                            result.new_track_name.as_ref().unwrap_or(&result.original_track.name)
                                                                        )
                                                                    }</strong>
                                                                    <div class="rules-applied">
                                                                        {"Rules: "} {result.rules_applied.join(", ")}
                                                                    </div>
                                                                </div>
                                                            }
                                                        } else {
                                                            html! { <div class="no-change">{"(no change)"}</div> }
                                                        }
                                                    }
                                                </div>
                                            </div>
                                        }
                                    }).collect::<Html>()
                                }
                            </div>
                        </div>
                    }
                } else {
                    html! {}
                }
            }
        </div>
    }
}
