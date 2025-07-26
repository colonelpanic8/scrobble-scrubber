use crate::types::AppState;
use crate::Route;
use dioxus::prelude::*;
use dioxus_router::prelude::*;

#[derive(Debug, Clone)]
struct NavItem {
    route: Route,
    label: &'static str,
}

impl PartialEq for NavItem {
    fn eq(&self, other: &Self) -> bool {
        std::mem::discriminant(&self.route) == std::mem::discriminant(&other.route)
    }
}

const NAV_ITEMS: &[NavItem] = &[
    NavItem {
        route: Route::ScrobbleScrubber {},
        label: "Scrubber",
    },
    NavItem {
        route: Route::RuleWorkshop {},
        label: "Rule Workshop",
    },
    NavItem {
        route: Route::RewriteRules {},
        label: "Rewrite Rules",
    },
    NavItem {
        route: Route::PendingEdits {},
        label: "Pending Edits",
    },
    NavItem {
        route: Route::PendingRules {},
        label: "Pending Rules",
    },
    NavItem {
        route: Route::CacheManagement {},
        label: "Cache Management",
    },
    NavItem {
        route: Route::MusicBrainz {},
        label: "MusicBrainz",
    },
    NavItem {
        route: Route::Config {},
        label: "Configuration",
    },
];

#[component]
pub fn Navigation(state: Signal<AppState>) -> Element {
    let current_route = use_route::<Route>();

    rsx! {
        nav {
            class: "nav-container",
            style: "background: white; border-radius: 0.5rem; box-shadow: 0 4px 6px rgba(0,0,0,0.1); padding: 1rem; margin-bottom: 1.5rem;",
            ul {
                class: "nav-list",
                style: "display: flex; justify-content: center; list-style: none; margin: 0; padding: 0; gap: 1rem; flex-wrap: wrap;",
                for item in NAV_ITEMS.iter() {
                    NavLink { item: item.clone(), current_route: current_route.clone() }
                }
            }
        }
    }
}

#[component]
fn NavLink(item: NavItem, current_route: Route) -> Element {
    let is_active = std::mem::discriminant(&current_route) == std::mem::discriminant(&item.route);

    let style = format!(
        "padding: 0.75rem 1.5rem; border: none; border-radius: 0.375rem; cursor: pointer; font-weight: 500; transition: all 0.2s; text-decoration: none; display: inline-block; {}",
        if is_active {
            "background: #2563eb; color: white;"
        } else {
            "background: #f3f4f6; color: #374151; hover: background-color: #e5e7eb;"
        }
    );

    rsx! {
        li {
            Link {
                to: item.route,
                style,
                aria_current: if is_active { "page" } else { "false" },
                {item.label}
            }
        }
    }
}
