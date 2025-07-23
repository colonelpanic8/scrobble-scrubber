use crate::types::{AppState, Page};
use dioxus::prelude::*;

#[component]
pub fn Navigation(mut state: Signal<AppState>) -> Element {
    let active_page = state.read().active_page.clone();

    rsx! {
        nav {
            style: "background: white; border-radius: 0.5rem; box-shadow: 0 4px 6px rgba(0,0,0,0.1); padding: 1rem; margin-bottom: 1.5rem;",
            ul {
                style: "display: flex; justify-content: center; list-style: none; margin: 0; padding: 0; gap: 1rem;",
                li {
                    button {
                        style: format!(
                            "padding: 0.75rem 1.5rem; border: none; border-radius: 0.375rem; cursor: pointer; font-weight: 500; transition: all 0.2s; {}",
                            if active_page == Page::RuleWorkshop {
                                "background: #2563eb; color: white;"
                            } else {
                                "background: #f3f4f6; color: #374151; hover:background: #e5e7eb;"
                            }
                        ),
                        onclick: move |_| {
                            state.with_mut(|s| s.active_page = Page::RuleWorkshop);
                        },
                        "Rule Workshop"
                    }
                }
                li {
                    button {
                        style: format!(
                            "padding: 0.75rem 1.5rem; border: none; border-radius: 0.375rem; cursor: pointer; font-weight: 500; transition: all 0.2s; {}",
                            if active_page == Page::RewriteRules {
                                "background: #2563eb; color: white;"
                            } else {
                                "background: #f3f4f6; color: #374151; hover:background: #e5e7eb;"
                            }
                        ),
                        onclick: move |_| {
                            state.with_mut(|s| s.active_page = Page::RewriteRules);
                        },
                        "Rewrite Rules"
                    }
                }
                li {
                    button {
                        style: format!(
                            "padding: 0.75rem 1.5rem; border: none; border-radius: 0.375rem; cursor: pointer; font-weight: 500; transition: all 0.2s; {}",
                            if active_page == Page::ScrobbleScrubber {
                                "background: #2563eb; color: white;"
                            } else {
                                "background: #f3f4f6; color: #374151; hover:background: #e5e7eb;"
                            }
                        ),
                        onclick: move |_| {
                            state.with_mut(|s| s.active_page = Page::ScrobbleScrubber);
                        },
                        "Scrobble Scrubber"
                    }
                }
                li {
                    button {
                        style: format!(
                            "padding: 0.75rem 1.5rem; border: none; border-radius: 0.375rem; cursor: pointer; font-weight: 500; transition: all 0.2s; {}",
                            if active_page == Page::PendingItems {
                                "background: #2563eb; color: white;"
                            } else {
                                "background: #f3f4f6; color: #374151; hover:background: #e5e7eb;"
                            }
                        ),
                        onclick: move |_| {
                            state.with_mut(|s| s.active_page = Page::PendingItems);
                        },
                        "Pending Items"
                    }
                }
                li {
                    button {
                        style: format!(
                            "padding: 0.75rem 1.5rem; border: none; border-radius: 0.375rem; cursor: pointer; font-weight: 500; transition: all 0.2s; {}",
                            if active_page == Page::CacheManagement {
                                "background: #2563eb; color: white;"
                            } else {
                                "background: #f3f4f6; color: #374151; hover:background: #e5e7eb;"
                            }
                        ),
                        onclick: move |_| {
                            state.with_mut(|s| s.active_page = Page::CacheManagement);
                        },
                        "Cache Management"
                    }
                }
            }
        }
    }
}
