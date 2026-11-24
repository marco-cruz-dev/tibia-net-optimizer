//! A tweak backed by one kernel tunable (Linux and macOS).

use super::{Tweak, TweakId, TweakState, TweakStatus};
use crate::unix::persist::{Entry, SettingsStore};
use crate::unix::sysctl::Sysctl;
use crate::Result;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SysctlSpec {
    pub id: TweakId,
    pub label: &'static str,
    pub key: &'static str,
    /// Value we want for Tibia.
    pub optimized: &'static str,
    /// OS default, used by `revert` when the original wasn't recorded.
    pub default: &'static str,
}

pub struct SysctlTweak<S, P> {
    pub spec: SysctlSpec,
    pub sysctl: S,
    pub store: P,
}

impl<S: Sysctl, P: SettingsStore> Tweak for SysctlTweak<S, P> {
    fn id(&self) -> TweakId {
        self.spec.id
    }
    fn label(&self) -> &'static str {
        self.spec.label
    }

    fn check(&self) -> TweakStatus {
        let SysctlSpec { key, optimized, .. } = self.spec;
        match self.sysctl.read(key) {
            Ok(value) => {
                let state = if value == optimized {
                    TweakState::Optimized
                } else {
                    TweakState::NotOptimized
                };
                TweakStatus::new(self.id(), self.label(), state, format!("{key} = {value}"))
            }
            Err(e) => TweakStatus::unknown(self.id(), self.label(), e),
        }
    }

    /// Writes the optimized value now and persists it for future boots,
    /// remembering the value it replaced (only the first time).
    fn apply(&self) -> Result<()> {
        let SysctlSpec { key, optimized, .. } = self.spec;
        let current = self.sysctl.read(key).ok();
        self.sysctl.write(key, optimized)?;

        let mut settings = self.store.load()?;
        let original = settings
            .entries
            .get(key)
            .and_then(|e| e.original.clone())
            .or(current.filter(|c| c != optimized));
        settings.entries.insert(
            key.to_string(),
            Entry {
                value: optimized.to_string(),
                original,
            },
        );
        self.store.save(&settings)
    }

    /// Restores the recorded original (or the OS default) and stops
    /// persisting this key.
    fn revert(&self) -> Result<()> {
        let SysctlSpec { key, default, .. } = self.spec;
        let mut settings = self.store.load()?;
        let original = settings
            .entries
            .remove(key)
            .and_then(|e| e.original)
            .unwrap_or_else(|| default.to_string());
        self.sysctl.write(key, &original)?;
        self.store.save(&settings)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::unix::persist::fake::MemStore;
    use crate::unix::persist::PersistedSettings;
    use crate::unix::sysctl::fake::FakeSysctl;

    const SPEC: SysctlSpec = SysctlSpec {
        id: TweakId::MacDelayedAck,
        label: "Delayed ACK",
        key: "net.inet.tcp.delayed_ack",
        optimized: "0",
        default: "3",
    };

    fn tweak<'a>(s: &'a FakeSysctl, p: &'a MemStore) -> SysctlTweak<&'a FakeSysctl, &'a MemStore> {
        SysctlTweak {
            spec: SPEC,
            sysctl: s,
            store: p,
        }
    }

    fn entry(store: &MemStore, key: &str) -> Option<Entry> {
        store.0.borrow().entries.get(key).cloned()
    }

    #[test]
    fn check_compares_against_optimized_value() {
        let store = MemStore::default();
        let sys = FakeSysctl::with(&[(SPEC.key, "3")]);
        let s = tweak(&sys, &store).check();
        assert_eq!(
            (s.id, s.state, s.summary.as_str()),
            (
                TweakId::MacDelayedAck,
                TweakState::NotOptimized,
                "net.inet.tcp.delayed_ack = 3"
            )
        );
        let sys = FakeSysctl::with(&[(SPEC.key, "0")]);
        assert_eq!(tweak(&sys, &store).check().state, TweakState::Optimized);
    }

    #[test]
    fn check_on_missing_key_is_unknown() {
        let s = tweak(&FakeSysctl::default(), &MemStore::default()).check();
        assert_eq!(s.state, TweakState::Unknown);
        assert!(s.summary.contains("unknown oid"), "{}", s.summary);
    }

    #[test]
    fn apply_writes_value_and_records_original() {
        let (sys, store) = (FakeSysctl::with(&[(SPEC.key, "2")]), MemStore::default());
        tweak(&sys, &store).apply().unwrap();
        assert_eq!(sys.get(SPEC.key).as_deref(), Some("0"));
        assert_eq!(
            entry(&store, SPEC.key),
            Some(Entry {
                value: "0".into(),
                original: Some("2".into())
            })
        );
    }

    #[test]
    fn reapply_keeps_the_first_original() {
        let (sys, store) = (FakeSysctl::with(&[(SPEC.key, "2")]), MemStore::default());
        let t = tweak(&sys, &store);
        t.apply().unwrap();
        sys.write(SPEC.key, "3").unwrap(); // OS reset it
        t.apply().unwrap();
        assert_eq!(entry(&store, SPEC.key).unwrap().original.as_deref(), Some("2"));
    }

    #[test]
    fn apply_when_already_optimized_records_no_original() {
        let (sys, store) = (FakeSysctl::with(&[(SPEC.key, "0")]), MemStore::default());
        tweak(&sys, &store).apply().unwrap();
        assert_eq!(entry(&store, SPEC.key).unwrap().original, None);
    }

    #[test]
    fn revert_restores_original_and_keeps_other_entries() {
        let (sys, store) = (FakeSysctl::with(&[(SPEC.key, "2")]), MemStore::default());
        store.0.borrow_mut().entries.insert(
            "other.key".into(),
            Entry {
                value: "0".into(),
                original: None,
            },
        );
        let t = tweak(&sys, &store);
        t.apply().unwrap();
        t.revert().unwrap();
        assert_eq!(sys.get(SPEC.key).as_deref(), Some("2"));
        assert_eq!(entry(&store, SPEC.key), None);
        assert!(entry(&store, "other.key").is_some());
    }

    #[test]
    fn revert_without_record_restores_os_default() {
        let (sys, store) = (FakeSysctl::with(&[(SPEC.key, "0")]), MemStore::default());
        tweak(&sys, &store).revert().unwrap();
        assert_eq!(sys.get(SPEC.key).as_deref(), Some("3"));
        assert_eq!(*store.0.borrow(), PersistedSettings::default());
    }
}
