//! What an exported track carries in its metadata.
//!
//! Pure: this decides the *values*, and `library::tracks::export_track` writes
//! them. Splitting it that way is what makes every rule below testable without
//! a FLAC on disk -- which album a track is tagged with, what happens when it is
//! in none, and where the year comes from.
//!
//! **The library's own copy is never tagged.** These values go onto the exported
//! copy only; the file under `tracks/` stays the byte-for-byte artifact its
//! provenance sidecar describes (ARCHITECTURE 8).

use crate::project::{AlbumList, ArtId, Project, TrackId};
use crate::provenance::Track;

/// The metadata one exported track should carry.
///
/// Every field is optional because every one of them can be genuinely absent --
/// an untitled track, a track in no album, an artist the user has not set. A
/// tag with nothing to say is left off rather than written empty: players show
/// an empty Artist field as a real value, and `""` is not what "unknown" means.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ExportTags {
    pub title: Option<String>,
    pub artist: Option<String>,
    pub album: Option<String>,
    /// 1-based position within `album`. Never set without `album`.
    pub track_number: Option<u32>,
    /// Four digits, from the track's own creation stamp.
    pub year: Option<String>,
    /// The artwork to embed, if one is resolvable.
    pub cover: Option<ArtId>,
    /// What made it -- model and licence, so the file says so away from the app.
    pub comment: Option<String>,
}

/// Trim to `None` when there is nothing left.
///
/// A user who clears the artist field leaves `Some("")` behind in config, and a
/// whitespace-only album name is not a name. Both must read as absent.
fn present(value: Option<&str>) -> Option<String> {
    let trimmed = value?.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

/// The album a track is tagged with, and its 1-based position in it.
///
/// **The first album containing the track, in the project's own order.** A
/// track can sit in several album lists, and there is no further fact to choose
/// between them: the export is of a track, not of an album, so nothing in the
/// request says which release is meant. First-in-project-order is at least
/// stable -- the user arranged that order -- where "any of them" would mean the
/// same export tagged differently on different days.
pub fn album_for<'a>(albums: &'a [AlbumList], id: &TrackId) -> Option<(&'a AlbumList, u32)> {
    albums.iter().find_map(|album| {
        let index = album.tracks.iter().position(|t| t == id)?;
        // A list is at most a few dozen tracks; the cast cannot lose anything a
        // person could have arranged by hand.
        Some((album, index as u32 + 1))
    })
}

/// The four-digit year of an RFC 3339 stamp, or `None` if it does not look like one.
///
/// Read as text rather than parsed into a date. The stamp is already the truth
/// (`library`'s `created` follows the same rule); parsing it into a local date
/// would reintroduce the timezone that turns a 1 January release into a
/// 31 December one.
pub fn year_of(created_at: &str) -> Option<String> {
    let year = created_at.get(..4)?;
    if year.len() == 4 && year.chars().all(|c| c.is_ascii_digit()) {
        Some(year.to_string())
    } else {
        None
    }
}

/// Everything an exported copy of `track` should be tagged with.
///
/// `artist` is the user's configured name; the rest is read from what the app
/// already holds. The cover falls back to the album's when the track has none of
/// its own -- a user who set one cover for a release did not mean "and no
/// artwork on the tracks".
pub fn tags_for(track: &Track, project: &Project, artist: Option<&str>) -> ExportTags {
    let album = album_for(&project.albums, &track.id);
    let album_name = album.and_then(|(a, _)| present(Some(&a.name)));

    ExportTags {
        title: present(track.title.as_deref()),
        artist: present(artist),
        // Guarded on the album's *name*, not on the album: a number with no
        // album to number within is meaningless in a player.
        track_number: album_name.as_ref().and(album).map(|(_, n)| n),
        album: album_name,
        year: year_of(&track.provenance.created_at),
        cover: track
            .cover
            .clone()
            .or_else(|| album.and_then(|(a, _)| a.cover.clone())),
        comment: Some(format!(
            "Generated with {} ({})",
            track.provenance.profile_display_name, track.provenance.model_license
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::generation::GenerationSpec;
    use crate::provenance::Provenance;
    use std::collections::BTreeMap;

    fn track(id: &str, title: Option<&str>, cover: Option<&str>, created_at: &str) -> Track {
        Track {
            id: TrackId(id.to_string()),
            title: title.map(str::to_string),
            cover: cover.map(|c| ArtId(c.to_string())),
            file: format!("tracks/{id}.flac"),
            duration_s: Some(120.0),
            provenance: Provenance {
                profile_id: "ace-step-1.5-turbo".to_string(),
                profile_display_name: "ACE-Step 1.5 XL Turbo".to_string(),
                model_license: "Apache-2.0".to_string(),
                spec: GenerationSpec {
                    title: None,
                    profile_id: "ace-step-1.5-turbo".to_string(),
                    inputs: BTreeMap::new(),
                    loras: vec![],
                    lyrics: None,
                },
                template: None,
                resolved_slots: BTreeMap::new(),
                comfy: None,
                created_at: created_at.to_string(),
                prompt_id: None,
            },
        }
    }

    fn project(albums: Vec<AlbumList>) -> Project {
        Project {
            slug: "night-drive".to_string(),
            name: "Night Drive".to_string(),
            created_at: "2026-08-30T10:00:00Z".to_string(),
            tracks: vec![],
            art: vec![],
            lyrics: vec![],
            albums,
            next_lyric_seq: 1,
            next_track_seq: 1,
            next_art_seq: 1,
        }
    }

    fn album(name: &str, tracks: &[&str], cover: Option<&str>) -> AlbumList {
        AlbumList {
            name: name.to_string(),
            tracks: tracks.iter().map(|t| TrackId(t.to_string())).collect(),
            cover: cover.map(|c| ArtId(c.to_string())),
        }
    }

    #[test]
    fn test_tags_read_the_title_album_number_and_year() {
        let t = track("tr-0002", Some("Midnight"), None, "2026-09-06T04:00:00Z");
        let p = project(vec![album("Night Drive", &["tr-0001", "tr-0002"], None)]);

        let tags = tags_for(&t, &p, Some("Rick"));

        assert_eq!(tags.title.as_deref(), Some("Midnight"));
        assert_eq!(tags.artist.as_deref(), Some("Rick"));
        assert_eq!(tags.album.as_deref(), Some("Night Drive"));
        assert_eq!(tags.track_number, Some(2));
        assert_eq!(tags.year.as_deref(), Some("2026"));
    }

    /// Invariant: an absent value is absent, never an empty tag. A player shows
    /// an empty Artist as a real value, so `""` would claim the artist is blank
    /// rather than unset.
    #[test]
    fn test_blank_and_whitespace_values_are_left_off_entirely() {
        let t = track("tr-0001", Some("   "), None, "2026-09-06T04:00:00Z");
        let p = project(vec![album("   ", &["tr-0001"], None)]);

        let tags = tags_for(&t, &p, Some("  "));

        assert_eq!(tags.title, None);
        assert_eq!(tags.artist, None);
        assert_eq!(tags.album, None);
        assert_eq!(
            tags.track_number, None,
            "a track number with no album to number within is meaningless"
        );
    }

    #[test]
    fn test_a_track_in_no_album_gets_no_album_or_number() {
        let t = track("tr-0001", Some("Loose"), None, "2026-09-06T04:00:00Z");
        let p = project(vec![album("Other", &["tr-0009"], None)]);

        let tags = tags_for(&t, &p, None);

        assert_eq!(tags.album, None);
        assert_eq!(tags.track_number, None);
        assert_eq!(tags.artist, None, "no artist configured is no artist tag");
        assert_eq!(tags.title.as_deref(), Some("Loose"));
    }

    /// Invariant: stable, not arbitrary. The same track exported twice must be
    /// tagged the same way, so the choice follows the project's own order.
    #[test]
    fn test_a_track_in_several_albums_takes_the_first_in_project_order() {
        let t = track("tr-0001", None, None, "2026-09-06T04:00:00Z");
        let p = project(vec![
            album("First", &["tr-0000", "tr-0001"], None),
            album("Second", &["tr-0001"], None),
        ]);

        let tags = tags_for(&t, &p, None);

        assert_eq!(tags.album.as_deref(), Some("First"));
        assert_eq!(tags.track_number, Some(2));
    }

    /// Invariant: a cover set on the release covers its tracks. Setting one
    /// album cover and no per-track covers is not a request for bare tracks.
    #[test]
    fn test_the_album_cover_stands_in_when_the_track_has_none() {
        let t = track("tr-0001", None, None, "2026-09-06T04:00:00Z");
        let p = project(vec![album("Night Drive", &["tr-0001"], Some("ar-0007"))]);

        assert_eq!(
            tags_for(&t, &p, None).cover,
            Some(ArtId("ar-0007".to_string()))
        );
    }

    #[test]
    fn test_the_tracks_own_cover_wins_over_the_albums() {
        let t = track("tr-0001", None, Some("ar-0001"), "2026-09-06T04:00:00Z");
        let p = project(vec![album("Night Drive", &["tr-0001"], Some("ar-0007"))]);

        assert_eq!(
            tags_for(&t, &p, None).cover,
            Some(ArtId("ar-0001".to_string()))
        );
    }

    #[test]
    fn test_the_comment_names_the_model_and_its_licence() {
        let t = track("tr-0001", None, None, "2026-09-06T04:00:00Z");
        let tags = tags_for(&t, &project(vec![]), None);

        let comment = tags.comment.unwrap();
        assert!(comment.contains("ACE-Step 1.5 XL Turbo"), "{comment}");
        assert!(comment.contains("Apache-2.0"), "{comment}");
    }

    /// Read as text, never parsed into a date: a timezone shift turns a
    /// 1 January release into a 31 December one.
    #[test]
    fn test_year_reads_the_stamp_and_refuses_anything_else() {
        assert_eq!(year_of("2026-09-06T04:00:00Z").as_deref(), Some("2026"));
        assert_eq!(year_of("2026-01-01T00:00:00Z").as_deref(), Some("2026"));
        assert_eq!(year_of(""), None);
        assert_eq!(year_of("nope"), None);
        assert_eq!(year_of("20x6-09-06"), None);
    }
}
