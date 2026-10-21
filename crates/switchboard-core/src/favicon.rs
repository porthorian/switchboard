use std::collections::BTreeSet;
use std::sync::Arc;

use crate::{BrowserState, PatchOp, ProfileId, TabId};

pub const MAX_FAVICON_BYTES: usize = 32 * 1024;
pub const PROFILE_FAVICON_BUDGET: usize = 8 * 1024 * 1024;
pub const GLOBAL_FAVICON_BUDGET: usize = 32 * 1024 * 1024;

/// Immutable, Rust-owned image data. Never serialized into the UI bridge.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Favicon {
    pub id: u64,
    pub profile_id: ProfileId,
    pub page_url: String,
    pub icon_url: String,
    pub png: Arc<[u8]>,
}

pub fn valid_favicon_png(png: &[u8]) -> bool {
    if png.len() < 33 || png.len() > MAX_FAVICON_BYTES
        || &png[..8] != b"\x89PNG\r\n\x1a\n"
        || png[8..12] != [0, 0, 0, 13] || &png[12..16] != b"IHDR"
    { return false; }
    let width = u32::from_be_bytes(png[16..20].try_into().unwrap());
    let height = u32::from_be_bytes(png[20..24].try_into().unwrap());
    (1..=64).contains(&width) && (1..=64).contains(&height)
}

pub fn page_key(url: &str) -> &str { url.split('#').next().unwrap_or(url) }

impl BrowserState {
    pub fn favicon_for_page(&self, profile_id: ProfileId, url: &str) -> Option<u64> {
        self.favicons.values().rev().find(|icon| {
            icon.profile_id == profile_id && icon.page_url == page_key(url)
        }).map(|icon| icon.id)
    }

    pub(crate) fn observe_favicon(&mut self, tab_id: TabId, page_url: String, icon_url: String, png: Vec<u8>, ops: &mut Vec<PatchOp>) {
        let Some(tab) = self.tabs.get(&tab_id) else { return; };
        if !tab.status.is_open() || page_key(&tab.url) != page_key(&page_url)
            || !(icon_url.starts_with("http://") || icon_url.starts_with("https://"))
            || !valid_favicon_png(&png) { return; }
        let profile_id = tab.profile_id;
        let page_url = page_key(&page_url).to_owned();
        if let Some(id) = self.favicons.values().find(|icon| icon.profile_id == profile_id && icon.page_url == page_url && icon.icon_url == icon_url && icon.png.as_ref() == png.as_slice()).map(|icon| icon.id) {
            if self.tabs[&tab_id].favicon_id != Some(id) {
                let tab = self.tabs.get_mut(&tab_id).unwrap();
                tab.favicon_id = Some(id);
                ops.push(PatchOp::UpsertTab(tab.clone()));
            }
            return;
        }
        let protected: BTreeSet<_> = self.tabs.values().filter_map(|tab| tab.favicon_id).collect();
        let mut removals = Vec::new();
        let mut total: usize = self.favicons.values().map(|icon| icon.png.len()).sum();
        let mut profile: usize = self.favicons.values().filter(|icon| icon.profile_id == profile_id).map(|icon| icon.png.len()).sum();
        for icon in self.favicons.values() {
            if total + png.len() <= GLOBAL_FAVICON_BUDGET && profile + png.len() <= PROFILE_FAVICON_BUDGET { break; }
            if protected.contains(&icon.id) { continue; }
            if profile + png.len() > PROFILE_FAVICON_BUDGET && icon.profile_id != profile_id { continue; }
            total -= icon.png.len();
            if icon.profile_id == profile_id { profile -= icon.png.len(); }
            removals.push(icon.id);
        }
        // Never remove usable icons merely to make room for a rejected candidate.
        if total + png.len() > GLOBAL_FAVICON_BUDGET || profile + png.len() > PROFILE_FAVICON_BUDGET { return; }
        for id in removals { self.favicons.remove(&id); }
        let id = self.allocate_favicon_id();
        let shared = self.favicons.values().find(|icon| icon.profile_id == profile_id && icon.icon_url == icon_url && icon.png.as_ref() == png.as_slice()).map(|icon| icon.png.clone()).unwrap_or_else(|| Arc::from(png));
        self.favicons.insert(id, Favicon { id, profile_id, page_url: page_url.clone(), icon_url, png: shared });
        // Refresh all tabs visiting this page without creating or restoring a view.
        for tab in self.tabs.values_mut().filter(|tab| tab.profile_id == profile_id && page_key(&tab.url) == page_url) {
            tab.favicon_id = Some(id);
            ops.push(PatchOp::UpsertTab(tab.clone()));
        }
    }

    pub(crate) fn clear_unused_favicons(&mut self, profile_id: Option<ProfileId>) {
        let referenced: BTreeSet<_> = self.tabs.values().filter_map(|tab| tab.favicon_id).collect();
        self.favicons.retain(|id, icon| profile_id.is_some_and(|profile| profile != icon.profile_id) || referenced.contains(id));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Engine, Intent, NoopPersistence};
    fn image() -> Vec<u8> {
        let mut png = vec![0; 33];
        png[..16].copy_from_slice(b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR");
        png[19] = 32; png[23] = 32; png
    }
    fn seeded() -> (Engine<NoopPersistence>, TabId) {
        let mut engine = Engine::new(NoopPersistence);
        engine.dispatch(Intent::NewProfile { name: "One".into() }).unwrap();
        engine.dispatch(Intent::NewTab { workspace_id: engine.state().active_workspace_id().unwrap(), url: Some("https://example.com".into()), make_active: true }).unwrap();
        let tab = *engine.state().tabs.keys().next().unwrap(); (engine, tab)
    }
    #[test]
    fn icons_are_profile_scoped_and_candidates_share_buffers() {
        let (mut engine, tab) = seeded();
        let observation = || Intent::ObserveTabFavicon { tab_id: tab, page_url: "https://example.com".into(), icon_url: "https://example.com/icon.png".into(), png: image() };
        let patch = engine.dispatch(observation()).unwrap();
        assert_eq!(patch.ops.len(), 1);
        let id = engine.state().tabs[&tab].favicon_id.unwrap();
        let cloned = engine.state().clone();
        assert!(Arc::ptr_eq(&engine.state().favicons[&id].png, &cloned.favicons[&id].png));
        assert!(engine.dispatch(observation()).unwrap().ops.is_empty());
        engine.dispatch(Intent::NewProfile { name: "Two".into() }).unwrap();
        assert_eq!(engine.state().favicon_for_page(engine.state().active_profile_id.unwrap(), "https://example.com"), None);
        let patch = engine.dispatch(Intent::ObserveTabFavicon { tab_id: tab, page_url: "https://old.example".into(), icon_url: "https://old.example/icon.png".into(), png: image() }).unwrap();
        assert!(patch.ops.is_empty());
    }
    #[test]
    fn cache_evicts_unreferenced_entries_before_referenced_icons() {
        let (engine, tab) = seeded();
        let mut state = engine.state().clone();
        let profile_id = state.active_profile_id.unwrap();
        let mut png = image(); png.resize(MAX_FAVICON_BYTES, 0);
        let png: Arc<[u8]> = png.into();
        for id in 1..=256 {
            state.favicons.insert(id, Favicon { id, profile_id, page_url: format!("https://cache.example/{id}"), icon_url: "https://cache.example/icon.png".into(), png: png.clone() });
        }
        state.tabs.get_mut(&tab).unwrap().favicon_id = Some(1);
        state.recompute_next_ids();
        let mut ops = Vec::new();
        state.observe_favicon(tab, "https://example.com".into(), "https://example.com/icon.png".into(), image(), &mut ops);
        assert!(state.favicons.contains_key(&1));
        assert!(!state.favicons.contains_key(&2));
        assert!(state.favicons.values().map(|icon| icon.png.len()).sum::<usize>() <= PROFILE_FAVICON_BUDGET);
        assert_eq!(state.tabs[&tab].favicon_id, Some(257));
    }
    #[test]
    fn clearing_history_keeps_tab_icons_and_removes_unused_cache() {
        let (mut engine, tab) = seeded();
        engine.dispatch(Intent::ObserveTabFavicon { tab_id: tab, page_url: "https://example.com".into(), icon_url: "https://example.com/icon.png".into(), png: image() }).unwrap();
        engine.dispatch(Intent::ClearHistory { scope: crate::ClearHistoryScope::All }).unwrap();
        assert_eq!(engine.state().favicons.len(), 1);
        engine.dispatch(Intent::Navigate { tab_id: tab, url: "https://new.example".into() }).unwrap();
        assert_eq!(engine.state().tabs[&tab].favicon_id, None);
        engine.dispatch(Intent::ClearHistory { scope: crate::ClearHistoryScope::All }).unwrap();
        assert!(engine.state().favicons.is_empty());
    }
    #[test]
    fn rejects_oversized_and_non_png_images() {
        assert!(!valid_favicon_png(b"<svg><script/></svg>"));
        let mut png = vec![0; 33];
        png[..16].copy_from_slice(b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR");
        png[19] = 32; png[23] = 32;
        assert!(valid_favicon_png(&png));
        png[19] = 65;
        assert!(!valid_favicon_png(&png));
        png[19] = 32; png.resize(MAX_FAVICON_BYTES + 1, 0);
        assert!(!valid_favicon_png(&png));
    }
}
