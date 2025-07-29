* Remember that 'variables can be used directly in the `format!` string'
* Make sure to run `cargo fmt --all` after finishing making changes
* Make sure to fix any clippy issues `cargo clippy --all-targets --all-features -- -D warnings` after making changes
* When making check to the dioxus app in app, check your changes by running `dx build`
* Always use the fully qualified name when logging (i.e. log::info!(...) over info!(...))
