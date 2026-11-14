//! Reference-counted values with an explicit cycle collector.
//!
//! Lists, dictionaries and Funcrefs are allocated behind `Rc<RefCell<...>>`
//! so aliasing (Vim's mutable containers) is cheap. `Rc` reclaims acyclic
//! garbage immediately; unreachable cycles are broken by the mark-sweep pass in
//! this module. The collector never frees a live value: it only clears objects
//! with no references from outside the tracked object graph, which is safe in
//! the presence of interpreter temporaries (they hold strong references).

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::{Rc, Weak};

use crate::value::Value;

/// Objects that can contain other Vim values and therefore participate in a
/// reference cycle.
pub(crate) trait GcTrace {
    /// Snapshot the strong references held by this object.
    fn trace(&self) -> Vec<Value>;
    /// Break all outgoing references. Called only for unreachable objects.
    fn clear(&self);
}

thread_local! {
    static REGISTRY: RefCell<Vec<Weak<dyn GcTrace>>> = const { RefCell::new(Vec::new()) };
    static COLLECTING: Cell<bool> = const { Cell::new(false) };
}

pub(crate) fn register<T: GcTrace + 'static>(object: &Rc<T>) {
    if COLLECTING.with(Cell::get) {
        return;
    }
    let trait_object: Rc<dyn GcTrace> = object.clone();
    let weak: Weak<dyn GcTrace> = Rc::downgrade(&trait_object);
    REGISTRY.with(|registry| registry.borrow_mut().push(weak));
}

fn weak_id(weak: &Weak<dyn GcTrace>) -> usize {
    weak.as_ptr() as *const () as usize
}

/// Number of tracked objects that are still alive (diagnostics and tests).
pub fn live_object_count() -> usize {
    REGISTRY.with(|registry| {
        registry
            .borrow()
            .iter()
            .filter(|w| w.strong_count() > 0)
            .count()
    })
}

/// Run the cycle collector.
///
/// The algorithm is a mark-sweep over tracked container objects:
/// 1. Count strong references held from other tracked objects.
/// 2. Any object with at least one reference from outside the tracked graph is
///    a root (runtime variables, call frames, Rust temporaries).
/// 3. Unmarked objects are exactly the members of unreachable cycles; clearing
///    their outgoing references breaks the cycles.
pub fn collect_cycles() {
    if COLLECTING.with(Cell::get) {
        return;
    }
    COLLECTING.with(|flag| flag.set(true));
    collect_inner();
    COLLECTING.with(|flag| flag.set(false));
    REGISTRY.with(|registry| registry.borrow_mut().retain(|weak| weak.strong_count() > 0));
}

fn collect_inner() {
    let weaks: Vec<Weak<dyn GcTrace>> = REGISTRY.with(|registry| registry.borrow().clone());
    let mut ids: Vec<usize> = Vec::new();
    let mut live: Vec<(Weak<dyn GcTrace>, usize)> = Vec::new();
    let mut positions: HashMap<usize, usize> = HashMap::new();
    for weak in weaks {
        let count = weak.strong_count();
        if count == 0 {
            continue;
        }
        let id = weak_id(&weak);
        if positions.contains_key(&id) {
            continue;
        }
        positions.insert(id, live.len());
        ids.push(id);
        live.push((weak, count));
    }
    if live.is_empty() {
        return;
    }

    // Trace each object once. The temporary clones returned by trace() are
    // dropped before the next object, and all strong counts were sampled first.
    let mut children: Vec<Vec<usize>> = vec![Vec::new(); live.len()];
    let mut internal = vec![0usize; live.len()];
    for (index, (weak, _)) in live.iter().enumerate() {
        let Some(object) = weak.upgrade() else {
            continue;
        };
        for child in object.trace() {
            if let Some(id) = child.object_id() {
                if let Some(&child_index) = positions.get(&id) {
                    internal[child_index] += 1;
                    children[index].push(child_index);
                }
            }
        }
    }

    let mut marked = vec![false; live.len()];
    let mut stack: Vec<usize> = Vec::new();
    for (index, (_, strong)) in live.iter().enumerate() {
        let external = strong.saturating_sub(internal[index]);
        if external > 0 {
            marked[index] = true;
            stack.push(index);
        }
    }
    while let Some(index) = stack.pop() {
        for &child in &children[index] {
            if !marked[child] {
                marked[child] = true;
                stack.push(child);
            }
        }
    }

    // Only unreachable cycles are cleared; live objects are never touched.
    for (index, (weak, _)) in live.iter().enumerate() {
        if !marked[index] {
            if let Some(object) = weak.upgrade() {
                object.clear();
            }
        }
    }
    let _ = ids;
}

/// Collection point invoked between top-level script statements.
pub(crate) fn maybe_collect_cycles() {
    let count = REGISTRY.with(|registry| registry.borrow().len());
    if count >= 1024 {
        collect_cycles();
    }
}
