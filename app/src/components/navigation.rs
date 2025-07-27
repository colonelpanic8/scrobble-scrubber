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
            class: "menubar-nav",
            style: "background: white; border-radius: 0.5rem; box-shadow: 0 4px 6px rgba(0,0,0,0.1); padding: 1rem; margin-bottom: 1.5rem;",
            role: "menubar",
            aria_label: "Main navigation",
            div {
                class: "menubar-container",
                style: "display: flex; justify-content: center; align-items: center; gap: 0.25rem;",
                for item in NAV_ITEMS.iter() {
                    MenubarItem {
                        item: item.clone(),
                        current_route: current_route.clone()
                    }
                }
            }
        }
    }
}

#[derive(Props, Clone, PartialEq)]
struct MenubarItemProps {
    item: NavItem,
    current_route: Route,
}

#[component]
fn MenubarItem(props: MenubarItemProps) -> Element {
    let navigator = use_navigator();
    let is_active =
        std::mem::discriminant(&props.current_route) == std::mem::discriminant(&props.item.route);

    rsx! {
        button {
            class: "menubar-item",
            role: "menuitem",
            tabindex: "0",
            style: format!(
                "padding: 0.75rem 1.5rem; border: none; border-radius: 0.375rem; cursor: pointer; font-weight: 500; transition: all 0.2s; margin: 0; {}",
                if is_active {
                    "background: #2563eb; color: white;"
                } else {
                    "background: #f3f4f6; color: #374151;"
                }
            ),
            onclick: {
                let route = props.item.route.clone();
                move |_| {
                    navigator.push(route.clone());
                }
            },
            onkeydown: {
                let route = props.item.route.clone();
                move |event: KeyboardEvent| {
                    let key = event.key();
                    if key == Key::Enter || key == Key::Character(" ".to_string()) {
                        navigator.push(route.clone());
                    }
                }
            },
            aria_current: if is_active { "page" } else { "false" },
            {props.item.label}
        }
    }
}
