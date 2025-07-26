use crate::types::AppState;
use crate::Route;
use dioxus::prelude::*;
use dioxus_router::prelude::*;

#[component]
pub fn Navigation(state: Signal<AppState>) -> Element {
    let current_route = use_route::<Route>();

    rsx! {
        nav {
            style: "background: white; border-radius: 0.5rem; box-shadow: 0 4px 6px rgba(0,0,0,0.1); padding: 1rem; margin-bottom: 1.5rem;",
            ul {
                style: "display: flex; justify-content: center; list-style: none; margin: 0; padding: 0; gap: 1rem;",
                li {
                    Link {
                        to: Route::ScrobbleScrubber {},
                        style: format!(
                            "padding: 0.75rem 1.5rem; border: none; border-radius: 0.375rem; cursor: pointer; font-weight: 500; transition: all 0.2s; text-decoration: none; display: inline-block; {}",
                            if matches!(current_route, Route::ScrobbleScrubber {}) {
                                "background: #2563eb; color: white;"
                            } else {
                                "background: #f3f4f6; color: #374151;"
                            }
                        ),
                        "Scrubber"
                    }
                }
                li {
                    Link {
                        to: Route::RuleWorkshop {},
                        style: format!(
                            "padding: 0.75rem 1.5rem; border: none; border-radius: 0.375rem; cursor: pointer; font-weight: 500; transition: all 0.2s; text-decoration: none; display: inline-block; {}",
                            if matches!(current_route, Route::RuleWorkshop {}) {
                                "background: #2563eb; color: white;"
                            } else {
                                "background: #f3f4f6; color: #374151;"
                            }
                        ),
                        "Rule Workshop"
                    }
                }
                li {
                    Link {
                        to: Route::RewriteRules {},
                        style: format!(
                            "padding: 0.75rem 1.5rem; border: none; border-radius: 0.375rem; cursor: pointer; font-weight: 500; transition: all 0.2s; text-decoration: none; display: inline-block; {}",
                            if matches!(current_route, Route::RewriteRules {}) {
                                "background: #2563eb; color: white;"
                            } else {
                                "background: #f3f4f6; color: #374151;"
                            }
                        ),
                        "Rewrite Rules"
                    }
                }
                li {
                    Link {
                        to: Route::PendingEdits {},
                        style: format!(
                            "padding: 0.75rem 1.5rem; border: none; border-radius: 0.375rem; cursor: pointer; font-weight: 500; transition: all 0.2s; text-decoration: none; display: inline-block; {}",
                            if matches!(current_route, Route::PendingEdits {}) {
                                "background: #2563eb; color: white;"
                            } else {
                                "background: #f3f4f6; color: #374151;"
                            }
                        ),
                        "Pending Edits"
                    }
                }
                li {
                    Link {
                        to: Route::PendingRules {},
                        style: format!(
                            "padding: 0.75rem 1.5rem; border: none; border-radius: 0.375rem; cursor: pointer; font-weight: 500; transition: all 0.2s; text-decoration: none; display: inline-block; {}",
                            if matches!(current_route, Route::PendingRules {}) {
                                "background: #2563eb; color: white;"
                            } else {
                                "background: #f3f4f6; color: #374151;"
                            }
                        ),
                        "Pending Rules"
                    }
                }
                li {
                    Link {
                        to: Route::CacheManagement {},
                        style: format!(
                            "padding: 0.75rem 1.5rem; border: none; border-radius: 0.375rem; cursor: pointer; font-weight: 500; transition: all 0.2s; text-decoration: none; display: inline-block; {}",
                            if matches!(current_route, Route::CacheManagement {}) {
                                "background: #2563eb; color: white;"
                            } else {
                                "background: #f3f4f6; color: #374151;"
                            }
                        ),
                        "Cache Management"
                    }
                }
            }
        }
    }
}
