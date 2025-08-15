pub mod client;
pub mod compilation_provider;
pub mod musicbrainz_provider;

pub use client::{MusicBrainzClient, MusicBrainzMatch};
pub use compilation_provider::{
    default_release_comparer, CompilationToCanonicalProvider, RankedRelease, ReleaseComparer,
};
pub use musicbrainz_provider::MusicBrainzScrubActionProvider;
