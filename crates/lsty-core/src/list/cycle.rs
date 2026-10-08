//! Dependency cycles among open items.

use super::snapshot::{Item, ItemId, Snapshot};

const UNSEEN: usize = usize::MAX;

/// Whether each item is part of a cycle: a path of one or more blocker
/// edges leads from it back to itself. Linear in items and edges.
pub(super) fn members(items: &[Item]) -> Vec<bool> {
    let n = items.len();
    let mut walk = Tarjan {
        items,
        number: vec![UNSEEN; n],
        low: vec![0; n],
        on_stack: vec![false; n],
        stack: Vec::new(),
        next: 0,
        cyclic: vec![false; n],
    };
    for root in 0..n {
        if walk.number.get(root) == Some(&UNSEEN) {
            walk.search(root);
        }
    }
    walk.cyclic
}

struct Tarjan<'a> {
    items: &'a [Item],
    number: Vec<usize>,
    low: Vec<usize>,
    on_stack: Vec<bool>,
    stack: Vec<usize>,
    next: usize,
    cyclic: Vec<bool>,
}

impl Tarjan<'_> {
    /// An iterative depth-first search from `root`: a frame is a node and
    /// the position of the blocker to look at next.
    fn search(&mut self, root: usize) {
        self.enter(root);
        let mut work = vec![(root, 0)];
        while let Some((node, pos)) = work.pop() {
            let Some(target) = self.blocker(node, pos) else {
                self.leave(node, work.last().map(|&(parent, _)| parent));
                continue;
            };
            work.push((node, pos + 1));
            if self.number.get(target) == Some(&UNSEEN) {
                self.enter(target);
                work.push((target, 0));
            } else if self.on_stack.get(target) == Some(&true) {
                self.lower(node, self.number.get(target).copied());
            }
        }
    }

    fn blocker(&self, node: usize, pos: usize) -> Option<usize> {
        let item = self.items.get(node)?;
        item.blockers.get(pos).map(|b| b.index())
    }

    fn enter(&mut self, node: usize) {
        if let Some(n) = self.number.get_mut(node) {
            *n = self.next;
        }
        if let Some(l) = self.low.get_mut(node) {
            *l = self.next;
        }
        if let Some(s) = self.on_stack.get_mut(node) {
            *s = true;
        }
        self.next += 1;
        self.stack.push(node);
    }

    fn lower(&mut self, node: usize, to: Option<usize>) {
        if let (Some(low), Some(to)) = (self.low.get_mut(node), to) {
            *low = (*low).min(to);
        }
    }

    /// Finishes `node`: it passes its low-link to its parent, and when it
    /// is the root of a component, that component is popped.
    fn leave(&mut self, node: usize, parent: Option<usize>) {
        let low = self.low.get(node).copied();
        if low == self.number.get(node).copied() {
            self.pop_component(node);
        }
        if let Some(parent) = parent {
            self.lower(parent, low);
        }
    }

    /// Pops the component rooted at `root`; it is a cycle when it has
    /// several members or its root depends on itself.
    fn pop_component(&mut self, root: usize) {
        let start = self.stack.iter().rposition(|&m| m == root);
        let start = start.unwrap_or(self.stack.len());
        let cyclic =
            self.stack.len() - start > 1 || self.depends_on_itself(root);
        for m in self.stack.drain(start..) {
            if let Some(s) = self.on_stack.get_mut(m) {
                *s = false;
            }
            if let (true, Some(c)) = (cyclic, self.cyclic.get_mut(m)) {
                *c = true;
            }
        }
    }

    fn depends_on_itself(&self, node: usize) -> bool {
        self.items
            .get(node)
            .is_some_and(|i| i.blockers.iter().any(|b| b.index() == node))
    }
}

impl Snapshot {
    /// The members of `item`'s dependency cycle in edge order, starting
    /// with `item`, or `None` when it is not part of one.
    pub fn cycle_path(&self, item: ItemId) -> Option<Vec<ItemId>> {
        let blockers = self.item(item)?.blockers();
        for &next in blockers {
            if let Some(mut path) = self.path_to(next, item) {
                path.pop();
                path.insert(0, item);
                return Some(path);
            }
        }
        None
    }

    /// The blocker path from one item to another, both included, or
    /// `None` when there is none. The search takes the blockers of each
    /// item in order and visits an item once.
    pub(crate) fn path_to(
        &self,
        from: ItemId,
        to: ItemId,
    ) -> Option<Vec<ItemId>> {
        let mut seen = vec![false; self.items.len()];
        let mut path = vec![(from, 0)];
        while let Some(&(node, pos)) = path.last() {
            if node == to {
                return Some(path.iter().map(|&(n, _)| n).collect());
            }
            if let Some(flag) = seen.get_mut(node.index()) {
                *flag = true;
            }
            match self.unseen_blocker(node, pos, &seen) {
                Some((at, next)) => descend(&mut path, at, next),
                None => {
                    path.pop();
                }
            }
        }
        None
    }

    /// The first blocker of `node` from position `pos` on that is not yet
    /// seen, with its position.
    fn unseen_blocker(
        &self,
        node: ItemId,
        pos: usize,
        seen: &[bool],
    ) -> Option<(usize, ItemId)> {
        let blockers = self.item(node)?.blockers();
        blockers
            .iter()
            .enumerate()
            .skip(pos)
            .find(|(_, b)| seen.get(b.index()) != Some(&true))
            .map(|(at, &b)| (at, b))
    }
}

/// Remembers that the top frame goes on after blocker `at`, and enters
/// `next`.
fn descend(path: &mut Vec<(ItemId, usize)>, at: usize, next: ItemId) {
    if let Some(top) = path.last_mut() {
        top.1 = at + 1;
    }
    path.push((next, 0));
}
