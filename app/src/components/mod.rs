pub mod cache_management;
pub mod config_page;
pub mod live_preview_controls;
pub mod login;
pub mod musicbrainz;
pub mod navigation;
pub mod pending_edits;
pub mod pending_rules;
pub mod rewrite_rules;
pub mod rule_editor;
pub mod rule_preview;
pub mod rule_workshop;
pub mod scrobble_scrubber;

pub use cache_management::CacheManagementPage;
pub use config_page::ConfigPage;
// pub use live_preview_controls::LivePreviewControls; // TODO: Use this when refactoring rule_workshop and rewrite_rules
pub use login::LoginPage;
pub use musicbrainz::MusicBrainzPage;
pub use navigation::Navigation;
pub use pending_edits::PendingEditsPage;
pub use pending_rules::PendingRulesPage;
pub use rewrite_rules::RewriteRulesPage;
pub use rule_editor::RuleEditor;
pub use rule_preview::RulePreview;
pub use rule_workshop::RuleWorkshop;
pub use scrobble_scrubber::{start_scrubber, ScrobbleScrubberPage};
