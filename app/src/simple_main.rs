use dioxus::prelude::*;

fn main() {
    dioxus::launch(App);
}

#[component]
fn App() -> Element {
    let mut logged_in = use_signal(|| false);

    rsx! {
        div { 
            style: "padding: 20px; font-family: Arial, sans-serif; max-width: 800px; margin: 0 auto;",
            
            h1 { 
                style: "text-align: center; color: #333; margin-bottom: 30px;",
                "Scrobble Rule Editor" 
            }
            
            if !*logged_in.read() {
                div {
                    style: "background: white; padding: 20px; border-radius: 8px; box-shadow: 0 2px 4px rgba(0,0,0,0.1);",
                    h2 { style: "margin-bottom: 20px;", "Login to Last.fm" }
                    
                    input { 
                        style: "width: 100%; padding: 8px; margin-bottom: 10px; border: 1px solid #ddd; border-radius: 4px;",
                        placeholder: "Username",
                        r#type: "text"
                    }
                    
                    input { 
                        style: "width: 100%; padding: 8px; margin-bottom: 15px; border: 1px solid #ddd; border-radius: 4px;",
                        placeholder: "Password",
                        r#type: "password"
                    }
                    
                    button { 
                        style: "background: #1976d2; color: white; padding: 10px 20px; border: none; border-radius: 4px; cursor: pointer;",
                        onclick: move |_| logged_in.set(true),
                        "Login (Mock)"
                    }
                }
            } else {
                div {
                    style: "background: white; padding: 20px; border-radius: 8px; box-shadow: 0 2px 4px rgba(0,0,0,0.1);",
                    h2 { style: "margin-bottom: 20px;", "Rule Workshop" }
                    
                    p { "Logged in successfully! This would show the rule editor." }
                    
                    button { 
                        style: "background: #d32f2f; color: white; padding: 8px 16px; border: none; border-radius: 4px; cursor: pointer;",
                        onclick: move |_| logged_in.set(false),
                        "Logout"
                    }
                }
            }
        }
    }
}