//! Dependency cycles among open tasks.

use super::snapshot::{Snapshot, Task, TaskId};

const UNSEEN: usize = usize::MAX;

/// Whether each task is part of a cycle: a path of one or more blocker
/// edges leads from it back to itself. Linear in tasks and edges.
pub(super) fn members(tasks: &[Task]) -> Vec<bool> {
    let n = tasks.len();
    let mut walk = Tarjan {
        tasks,
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
    tasks: &'a [Task],
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
        let task = self.tasks.get(node)?;
        task.blockers.get(pos).map(|b| b.index())
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
        self.tasks
            .get(node)
            .is_some_and(|i| i.blockers.iter().any(|b| b.index() == node))
    }
}

impl Snapshot {
    /// The members of `task`'s dependency cycle in edge order, starting
    /// with `task`, or `None` when it is not part of one.
    pub fn cycle_path(&self, task: TaskId) -> Option<Vec<TaskId>> {
        let blockers = self.task(task)?.blockers();
        for &next in blockers {
            if let Some(mut path) = self.path_to(next, task) {
                path.pop();
                path.insert(0, task);
                return Some(path);
            }
        }
        None
    }

    /// Whether each task is `to` or waits for it, at any depth, by task
    /// index. One walk over the reverse blocker edges.
    pub fn waiting_on(&self, to: TaskId) -> Vec<bool> {
        let mut waiters = vec![Vec::new(); self.tasks.len()];
        let edges = self.tasks.iter().flat_map(|i| {
            i.blockers().iter().map(move |b| (b.index(), i.task_id()))
        });
        for (b, waiter) in edges {
            waiters.get_mut(b).into_iter().for_each(|w| w.push(waiter));
        }
        let mut found = vec![false; self.tasks.len()];
        let mut stack = vec![to];
        while let Some(at) = stack.pop() {
            if let Some(seen) = found.get_mut(at.index())
                && !std::mem::replace(seen, true)
            {
                stack.extend(waiters.get(at.index()).into_iter().flatten());
            }
        }
        found
    }

    /// The blocker path from one task to another, both included, or
    /// `None` when there is none. The search takes the blockers of each
    /// task in order and visits a task once.
    pub(crate) fn path_to(
        &self,
        from: TaskId,
        to: TaskId,
    ) -> Option<Vec<TaskId>> {
        let mut seen = vec![false; self.tasks.len()];
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
        node: TaskId,
        pos: usize,
        seen: &[bool],
    ) -> Option<(usize, TaskId)> {
        let blockers = self.task(node)?.blockers();
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
fn descend(path: &mut Vec<(TaskId, usize)>, at: usize, next: TaskId) {
    if let Some(top) = path.last_mut() {
        top.1 = at + 1;
    }
    path.push((next, 0));
}
