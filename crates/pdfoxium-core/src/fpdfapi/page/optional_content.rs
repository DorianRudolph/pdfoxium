// Copyright 2026 The pdfoxium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
//! PDF Reference 1.7 §4.10 configuration and interactive state. Unlike a PDF
//! configuration, this state is transient. The model contains no document/Rc
//! pointers, so immutable snapshots can accompany threaded render requests.
use super::cpdf_occontext::UsageType;
use crate::fpdfapi::parser::{
    cpdf_array::CpdfArray, cpdf_dictionary::CpdfDictionary, cpdf_object::ObjectRef,
};
use std::{collections::HashSet, sync::Arc};

fn entry(o: &ObjectRef, key: &[u8]) -> Option<ObjectRef> {
    o.borrow().as_dictionary()?.get_direct_object_for(key)
}
fn items(o: &ObjectRef) -> Vec<ObjectRef> {
    let b = o.borrow();
    let Some(a) = b.as_any().downcast_ref::<CpdfArray>() else {
        return Vec::new();
    };
    (0..a.size())
        .filter_map(|i| a.get_direct_object_at(i))
        .collect()
}
fn list(o: &ObjectRef, key: &[u8]) -> Vec<ObjectRef> {
    entry(o, key).map_or_else(Vec::new, |v| items(&v))
}
fn name(o: &ObjectRef, key: &[u8]) -> Vec<u8> {
    entry(o, key).map_or_else(Vec::new, |v| v.borrow().get_string())
}
fn text(o: &ObjectRef, key: &[u8]) -> String {
    let wide = o
        .borrow()
        .as_dictionary()
        .map(|d| d.get_unicode_text_for(key))
        .unwrap_or_default();
    String::from_utf8_lossy(&crate::fxcrt::fx_string::utf8_encode(&wide)).into_owned()
}
fn intents(o: &ObjectRef, default: bool) -> Vec<Vec<u8>> {
    match entry(o, b"Intent") {
        None if default => vec![b"View".to_vec()],
        None => Vec::new(),
        Some(v) => {
            if v.borrow().as_any().is::<CpdfArray>() {
                items(&v).iter().map(|x| x.borrow().get_string()).collect()
            } else {
                vec![v.borrow().get_string()]
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum OcOrder {
    Group {
        index: usize,
        children: Vec<OcOrder>,
    },
    Label {
        text: String,
        children: Vec<OcOrder>,
    },
}
#[derive(Clone, Debug, PartialEq)]
pub struct OcGroup {
    pub object: u32,
    pub name: String,
    pub intents: Vec<Vec<u8>>,
    usage: Usage,
}
#[derive(Clone, Debug, Default, PartialEq)]
struct Usage {
    view: Option<bool>,
    print: Option<bool>,
    export: Option<bool>,
    zoom: Option<(f32, f32)>,
    language: Option<(String, bool)>,
    user: Option<(Vec<u8>, Vec<String>)>,
}
#[derive(Clone, Debug, PartialEq)]
struct Application {
    event: Vec<u8>,
    groups: Vec<usize>,
    categories: Vec<Vec<u8>>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct OcConfiguration {
    pub name: String,
    pub creator: String,
    pub order: Vec<OcOrder>,
    pub locked: Vec<usize>,
    pub radio_groups: Vec<Vec<usize>>,
    base: Vec<u8>,
    on: Vec<usize>,
    off: Vec<usize>,
    intents: Vec<Vec<u8>>,
    applications: Vec<Application>,
    pub list_all_pages: bool,
}
#[derive(Clone, Debug, PartialEq)]
pub struct OcModel {
    pub groups: Vec<OcGroup>,
    pub configurations: Vec<OcConfiguration>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct OcEnvironment {
    pub zoom: f32,
    pub language: String,
    pub user: Option<(Vec<u8>, String)>,
    pub automatic: bool,
}
impl Default for OcEnvironment {
    fn default() -> Self {
        Self {
            zoom: 1.,
            language: String::new(),
            user: None,
            automatic: true,
        }
    }
}
#[derive(Clone, Debug, PartialEq)]
pub struct OcSnapshot {
    pub revision: u64,
    states: Arc<[(u32, bool)]>,
}
impl OcSnapshot {
    pub fn visible(&self, object: u32) -> Option<bool> {
        self.states
            .binary_search_by_key(&object, |x| x.0)
            .ok()
            .map(|i| self.states[i].1)
    }
}
#[derive(Clone, Debug, PartialEq)]
pub struct OcState {
    pub model: Arc<OcModel>,
    pub configuration: usize,
    pub revision: u64,
    pub environment: OcEnvironment,
    states: Vec<bool>,
    overrides: Vec<bool>,
    snapshot: OcSnapshot,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OcOperation {
    On,
    Off,
    Toggle,
}
#[derive(Clone, Debug, PartialEq)]
pub struct OcAction {
    pub changes: Vec<(u32, OcOperation)>,
    pub preserve_radio: bool,
}
pub fn parse_action(dictionary: &ObjectRef) -> Option<OcAction> {
    let preserve_radio = dictionary
        .borrow()
        .as_dictionary()?
        .get_boolean_for(b"PreserveRB", true);
    let values = items(&entry(dictionary, b"State")?);
    let mut operation = None;
    let mut changes = Vec::new();
    for value in values {
        let b = value.borrow();
        if b.as_dictionary().is_some() {
            let object = b.obj_num();
            if object == 0 {
                return None;
            }
            changes.push((object, operation?));
        } else {
            operation = Some(match b.get_string().as_slice() {
                b"ON" => OcOperation::On,
                b"OFF" => OcOperation::Off,
                b"Toggle" => OcOperation::Toggle,
                _ => return None,
            });
        }
    }
    Some(OcAction {
        changes,
        preserve_radio,
    })
}

fn group_index(groups: &[OcGroup], o: &ObjectRef) -> Option<usize> {
    let n = o.borrow().obj_num();
    groups.iter().position(|g| g.object == n)
}
fn indices(groups: &[OcGroup], o: &ObjectRef, key: &[u8]) -> Vec<usize> {
    list(o, key)
        .iter()
        .filter_map(|v| group_index(groups, v))
        .collect()
}
fn order(
    groups: &[OcGroup],
    values: &[ObjectRef],
    depth: usize,
    path: &mut HashSet<usize>,
) -> Vec<OcOrder> {
    if depth > 32 {
        return Vec::new();
    };
    let mut out = Vec::new();
    for v in values {
        if let Some(index) = group_index(groups, v) {
            out.push(OcOrder::Group {
                index,
                children: Vec::new(),
            });
            continue;
        }
        if !v.borrow().as_any().is::<CpdfArray>() {
            continue;
        }
        let key = std::rc::Rc::as_ptr(v) as *const () as usize;
        if !path.insert(key) {
            continue;
        }
        let children = items(v);
        if children.first().is_some_and(|x| {
            x.borrow()
                .as_any()
                .is::<crate::fpdfapi::parser::cpdf_string::CpdfString>()
        }) {
            let title = children[0].borrow().get_unicode_text();
            let title =
                String::from_utf8_lossy(&crate::fxcrt::fx_string::utf8_encode(&title)).into_owned();
            out.push(OcOrder::Label {
                text: title,
                children: order(groups, &children[1..], depth + 1, path),
            });
        } else {
            let nested = order(groups, &children, depth + 1, path);
            if let Some(OcOrder::Group { children, .. }) = out.last_mut() {
                children.extend(nested)
            } else {
                out.extend(nested)
            }
        }
        path.remove(&key);
    }
    out
}
impl OcModel {
    pub fn load(root: Option<ObjectRef>) -> Option<Arc<Self>> {
        let props = entry(&root?, b"OCProperties")?;
        let mut groups = Vec::new();
        for o in list(&props, b"OCGs") {
            let object = o.borrow().obj_num();
            if object == 0
                || name(&o, b"Type") != b"OCG"
                || groups.iter().any(|g: &OcGroup| g.object == object)
            {
                continue;
            }
            let mut usage = Usage::default();
            if let Some(u) = entry(&o, b"Usage") {
                for (category, key, out) in [
                    (b"View".as_slice(), b"ViewState".as_slice(), &mut usage.view),
                    (b"Print", b"PrintState", &mut usage.print),
                    (b"Export", b"ExportState", &mut usage.export),
                ] {
                    *out = entry(&u, category).and_then(|d| match name(&d, key).as_slice() {
                        b"ON" => Some(true),
                        b"OFF" => Some(false),
                        _ => None,
                    });
                }
                if let Some(z) = entry(&u, b"Zoom") {
                    let b = z.borrow();
                    if let Some(d) = b.as_dictionary() {
                        let min = if d.key_exist(b"min") {
                            d.get_float_for(b"min")
                        } else {
                            0.
                        };
                        let max = if d.key_exist(b"max") {
                            d.get_float_for(b"max")
                        } else {
                            f32::INFINITY
                        };
                        usage.zoom = Some((min, max));
                    }
                }
                if let Some(l) = entry(&u, b"Language") {
                    usage.language = Some((text(&l, b"Lang"), name(&l, b"Preferred") == b"ON"));
                }
                if let Some(u) = entry(&u, b"User") {
                    let names = entry(&u, b"Name").map_or_else(Vec::new, |v| {
                        if v.borrow().as_any().is::<CpdfArray>() {
                            items(&v)
                        } else {
                            vec![v]
                        }
                    });
                    usage.user = Some((
                        name(&u, b"Type"),
                        names
                            .iter()
                            .map(|v| {
                                String::from_utf8_lossy(&crate::fxcrt::fx_string::utf8_encode(
                                    &v.borrow().get_unicode_text(),
                                ))
                                .into_owned()
                            })
                            .collect(),
                    ));
                }
            }
            groups.push(OcGroup {
                object,
                name: text(&o, b"Name"),
                intents: intents(&o, true),
                usage,
            });
        }
        if groups.is_empty() {
            return None;
        }
        let default = entry(&props, b"D")
            .filter(|d| d.borrow().as_dictionary().is_some())
            .unwrap_or_else(|| {
                crate::fpdfapi::parser::cpdf_object::retain_object(CpdfDictionary::new())
            });
        let mut configurations = Vec::new();
        for c in std::iter::once(default).chain(list(&props, b"Configs")) {
            if c.borrow().as_dictionary().is_none() {
                continue;
            }
            let applications = list(&c, b"AS")
                .iter()
                .filter_map(|a| {
                    a.borrow().as_dictionary()?;
                    Some(Application {
                        event: name(a, b"Event"),
                        groups: indices(&groups, a, b"OCGs"),
                        categories: list(a, b"Category")
                            .iter()
                            .map(|c| c.borrow().get_string())
                            .collect(),
                    })
                })
                .collect();
            configurations.push(OcConfiguration {
                name: text(&c, b"Name"),
                creator: text(&c, b"Creator"),
                order: order(&groups, &list(&c, b"Order"), 0, &mut HashSet::new()),
                locked: indices(&groups, &c, b"Locked"),
                radio_groups: list(&c, b"RBGroups")
                    .iter()
                    .map(|r| {
                        items(r)
                            .iter()
                            .filter_map(|g| group_index(&groups, g))
                            .collect()
                    })
                    .collect(),
                base: name(&c, b"BaseState"),
                on: indices(&groups, &c, b"ON"),
                off: indices(&groups, &c, b"OFF"),
                intents: intents(&c, true),
                applications,
                list_all_pages: name(&c, b"ListMode") != b"VisiblePages",
            });
        }
        for (index, c) in list(&props, b"Configs")
            .iter()
            .filter(|c| c.borrow().as_dictionary().is_some())
            .enumerate()
        {
            let target = index + 1;
            if entry(c, b"Order").is_none() {
                configurations[target].order = configurations[0].order.clone();
            }
            if entry(c, b"RBGroups").is_none() {
                configurations[target].radio_groups = configurations[0].radio_groups.clone();
            }
        }
        Some(Arc::new(Self {
            groups,
            configurations,
        }))
    }
}
impl OcState {
    pub fn new(model: Arc<OcModel>) -> Self {
        let count = model.groups.len();
        let mut s = Self {
            model,
            configuration: 0,
            revision: 0,
            environment: OcEnvironment::default(),
            states: vec![true; count],
            overrides: vec![false; count],
            snapshot: OcSnapshot {
                revision: 0,
                states: Arc::from([]),
            },
        };
        s.apply_configuration(0)
            .expect("model has default configuration");
        s
    }
    fn rebuild(&mut self) {
        self.revision = self.revision.saturating_add(1);
        let intents = &self.model.configurations[self.configuration].intents;
        let mut states: Vec<_> = self
            .model
            .groups
            .iter()
            .zip(&self.states)
            .map(|(g, &state)| {
                let applies = g
                    .intents
                    .iter()
                    .any(|i| i == b"All" || intents.iter().any(|j| j == b"All" || i == j));
                (g.object, !applies || state)
            })
            .collect();
        states.sort_unstable_by_key(|x| x.0);
        self.snapshot = OcSnapshot {
            revision: self.revision,
            states: states.into(),
        };
    }
    pub fn visible(&self, index: usize) -> Option<bool> {
        self.states.get(index).copied()
    }
    pub fn set_visible(
        &mut self,
        index: usize,
        on: bool,
        user: bool,
        preserve_radio: bool,
    ) -> Result<bool, &'static str> {
        if index >= self.states.len() {
            return Err("unknown optional content group");
        }
        let c = &self.model.configurations[self.configuration];
        if user && c.locked.contains(&index) {
            return Err("optional content group is locked");
        }
        if user
            && on
            && preserve_radio
            && c.radio_groups.iter().any(|radio| {
                radio.contains(&index)
                    && radio.iter().any(|&other| {
                        other != index && self.states[other] && c.locked.contains(&other)
                    })
            })
        {
            return Err("a radio group peer is locked on");
        }
        let mut changed = self.states[index] != on;
        let mut context_changed = !self.overrides[index];
        self.states[index] = on;
        self.overrides[index] = true;
        if on && preserve_radio {
            for radio in &c.radio_groups {
                if radio.contains(&index) {
                    for &other in radio {
                        if other != index {
                            changed |= self.states[other];
                            context_changed |= !self.overrides[other];
                            self.states[other] = false;
                            self.overrides[other] = true;
                        }
                    }
                }
            }
        }
        if changed || context_changed {
            self.rebuild()
        }
        Ok(changed)
    }
    pub fn apply_configuration(&mut self, index: usize) -> Result<(), &'static str> {
        let c = self
            .model
            .configurations
            .get(index)
            .ok_or("unknown optional content configuration")?;
        if c.base != b"Unchanged" {
            self.states.fill(c.base != b"OFF")
        }
        for &i in &c.on {
            self.states[i] = true
        }
        for &i in &c.off {
            self.states[i] = false
        }
        self.configuration = index;
        self.overrides.fill(false);
        self.auto_state(UsageType::View);
        self.rebuild();
        Ok(())
    }
    fn auto_state(&mut self, event: UsageType) {
        if !self.environment.automatic {
            return;
        }
        let event = match event {
            UsageType::Print => b"Print".as_slice(),
            UsageType::Export => b"Export",
            _ => b"View",
        };
        let mut recommendations = vec![None; self.states.len()];
        for a in &self.model.configurations[self.configuration].applications {
            if a.event != event {
                continue;
            }
            let exact = a.groups.iter().any(|&i| {
                self.model.groups[i]
                    .usage
                    .language
                    .as_ref()
                    .is_some_and(|(lang, _)| lang.eq_ignore_ascii_case(&self.environment.language))
            });
            for &i in &a.groups {
                if event == b"View" && self.overrides[i] {
                    continue;
                }
                let u = &self.model.groups[i].usage;
                for category in &a.categories {
                    let recommendation = match category.as_slice() {
                        b"View" => u.view,
                        b"Print" => u.print,
                        b"Export" => u.export,
                        b"Zoom" => u.zoom.map(|(min, max)| {
                            self.environment.zoom >= min && self.environment.zoom < max
                        }),
                        b"Language" => u.language.as_ref().map(|(lang, preferred)| {
                            if exact {
                                lang.eq_ignore_ascii_case(&self.environment.language)
                            } else {
                                *preferred
                                    && lang.split('-').next().unwrap_or("").eq_ignore_ascii_case(
                                        self.environment.language.split('-').next().unwrap_or(""),
                                    )
                            }
                        }),
                        b"User" => u.user.as_ref().map(|(kind, names)| {
                            self.environment
                                .user
                                .as_ref()
                                .is_some_and(|(k, n)| kind == k && names.contains(n))
                        }),
                        _ => None,
                    };
                    if let Some(on) = recommendation {
                        recommendations[i] = Some(recommendations[i].unwrap_or(true) && on)
                    }
                }
            }
        }
        for (i, on) in recommendations.into_iter().enumerate() {
            if let Some(on) = on {
                self.states[i] = on
            }
        }
    }
    pub fn set_environment(&mut self, environment: OcEnvironment) -> Result<bool, &'static str> {
        if !environment.zoom.is_finite() || environment.zoom <= 0. {
            return Err("optional content zoom must be finite and positive");
        }
        if self.environment == environment {
            return Ok(false);
        }
        self.environment = environment;
        let before = self.states.clone();
        self.auto_state(UsageType::View);
        let changed = before != self.states;
        // Context subscribers also need environment/override changes whose
        // current pixels happen to be identical.
        self.rebuild();
        Ok(changed)
    }
    pub fn apply_action(&mut self, action: &OcAction) -> Result<bool, &'static str> {
        let mut updated = self.clone();
        let mut changed = false;
        for &(object, operation) in &action.changes {
            let Some(index) = updated.model.groups.iter().position(|g| g.object == object) else {
                continue;
            };
            let on = match operation {
                OcOperation::On => true,
                OcOperation::Off => false,
                OcOperation::Toggle => !updated.states[index],
            };
            changed |= updated.set_visible(index, on, false, action.preserve_radio)?;
        }
        *self = updated;
        Ok(changed)
    }
    pub fn reset(&mut self) {
        self.states.fill(true);
        let _ = self.apply_configuration(0);
    }
    pub fn snapshot(&self, event: UsageType) -> OcSnapshot {
        if matches!(event, UsageType::View | UsageType::Design) {
            return self.snapshot.clone();
        }
        let mut temporary = self.clone();
        temporary.auto_state(event);
        temporary.rebuild();
        temporary.snapshot
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn state() -> OcState {
        let groups = (0..4)
            .map(|i| OcGroup {
                object: i + 10,
                name: format!("layer {i}"),
                intents: vec![b"View".to_vec()],
                usage: Usage::default(),
            })
            .collect();
        let config = OcConfiguration {
            name: "Default".into(),
            creator: String::new(),
            order: vec![],
            locked: vec![],
            radio_groups: vec![],
            base: b"ON".to_vec(),
            on: vec![],
            off: vec![1],
            intents: vec![b"View".to_vec()],
            applications: vec![],
            list_all_pages: true,
        };
        OcState::new(Arc::new(OcModel {
            groups,
            configurations: vec![config],
        }))
    }
    #[test]
    fn radio_locked_changes_are_atomic_and_programmatic_actions_ignore_lock() {
        let mut s = state();
        let m = Arc::make_mut(&mut s.model);
        m.configurations[0].locked = vec![0];
        m.configurations[0].radio_groups = vec![vec![0, 1]];
        let before = s.clone();
        assert!(s.set_visible(0, false, true, true).is_err());
        assert!(s.set_visible(1, true, true, true).is_err());
        assert_eq!(s, before);
        s.apply_action(&OcAction {
            changes: vec![(11, OcOperation::On)],
            preserve_radio: true,
        })
        .unwrap();
        assert_eq!((s.visible(0), s.visible(1)), (Some(false), Some(true)));
        s.apply_action(&OcAction {
            changes: vec![(10, OcOperation::Toggle)],
            preserve_radio: false,
        })
        .unwrap();
        assert_eq!((s.visible(0), s.visible(1)), (Some(true), Some(true)));
    }
    #[test]
    fn setting_an_already_on_group_publishes_its_new_manual_override() {
        let mut s = state();
        let before = s.snapshot(UsageType::View);
        assert!(!s.set_visible(0, true, true, true).unwrap());
        assert!(s.snapshot(UsageType::View).revision > before.revision);
        let mut e = s.environment.clone();
        e.zoom = 2.;
        let before = s.revision;
        assert!(!s.set_environment(e).unwrap());
        assert!(s.revision > before);
    }
    #[test]
    fn usage_requires_as_and_combines_categories_and_applications() {
        let mut s = state();
        Arc::make_mut(&mut s.model).groups[0].usage.view = Some(false);
        s.reset();
        assert_eq!(s.visible(0), Some(true)); // Metadata alone has no effect.
        let m = Arc::make_mut(&mut s.model);
        m.groups[0].usage.zoom = Some((1., 2.));
        m.configurations[0].applications = vec![Application {
            event: b"View".to_vec(),
            groups: vec![0],
            categories: vec![b"Zoom".to_vec()],
        }];
        s.reset();
        assert_eq!(s.visible(0), Some(true));
        let mut e = s.environment.clone();
        e.zoom = 2.;
        assert!(s.set_environment(e).unwrap());
        assert_eq!(s.visible(0), Some(false)); // Exclusive maximum.
        s.set_visible(0, true, true, true).unwrap();
        let mut e = s.environment.clone();
        e.zoom = 0.5;
        assert!(!s.set_environment(e).unwrap()); // Manual override persists.
        s.reset();
        assert_eq!(s.visible(0), Some(false));
        Arc::make_mut(&mut s.model).configurations[0]
            .applications
            .push(Application {
                event: b"View".to_vec(),
                groups: vec![0],
                categories: vec![b"View".to_vec()],
            });
        let mut e = s.environment.clone();
        e.zoom = 1.5;
        s.set_environment(e).unwrap();
        assert_eq!(s.visible(0), Some(false));
    }
    #[test]
    fn event_snapshots_are_temporary_and_intent_is_independent_of_event() {
        let mut s = state();
        let m = Arc::make_mut(&mut s.model);
        m.groups[0].usage.print = Some(false);
        m.groups[1].intents = vec![b"Design".to_vec()];
        m.configurations[0].applications.push(Application {
            event: b"Print".to_vec(),
            groups: vec![0],
            categories: vec![b"Print".to_vec()],
        });
        s.reset();
        let before = s.snapshot(UsageType::View);
        assert_eq!(before.visible(11), Some(true)); // Intent does not match.
        assert_eq!(s.snapshot(UsageType::Print).visible(10), Some(false));
        assert_eq!(s.snapshot(UsageType::View), before);
        s.environment.automatic = false;
        assert_eq!(s.snapshot(UsageType::Print).visible(10), Some(true));
    }
    #[test]
    fn language_exact_wins_then_preferred_partial_and_invalid_environment_is_atomic() {
        let mut s = state();
        let m = Arc::make_mut(&mut s.model);
        m.groups[0].usage.language = Some(("en-US".into(), false));
        m.groups[1].usage.language = Some(("en-GB".into(), true));
        m.configurations[0].applications.push(Application {
            event: b"View".to_vec(),
            groups: vec![0, 1],
            categories: vec![b"Language".to_vec()],
        });
        let mut e = s.environment.clone();
        e.language = "en-US".into();
        s.set_environment(e.clone()).unwrap();
        assert_eq!((s.visible(0), s.visible(1)), (Some(true), Some(false)));
        e.language = "en-AU".into();
        s.set_environment(e).unwrap();
        assert_eq!((s.visible(0), s.visible(1)), (Some(false), Some(true)));
        let before = s.clone();
        let mut e = s.environment.clone();
        e.zoom = f32::NAN;
        assert!(s.set_environment(e).is_err());
        assert_eq!(s, before);
        let mut alternative = s.model.configurations[0].clone();
        alternative.base = b"Unchanged".to_vec();
        alternative.on = vec![2];
        alternative.off = vec![3];
        alternative.applications.clear();
        Arc::make_mut(&mut s.model).configurations.push(alternative);
        s.apply_configuration(1).unwrap();
        assert_eq!(
            (s.visible(0), s.visible(1), s.visible(2), s.visible(3)),
            (Some(false), Some(true), Some(true), Some(false))
        );
        assert!(s.apply_configuration(99).is_err());
    }
}
