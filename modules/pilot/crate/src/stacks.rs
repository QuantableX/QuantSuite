//! Stacked pull requests (round 4, card 6): chains found from each change's
//! base branch, and one step that updates a whole chain.
//!
//! The update never touches the user's checkout or branches while it can still
//! fail: every rebase runs on detached commits in a worktree of its own under
//! the system's temp folder, and only when all of them worked does one atomic
//! push with `--force-with-lease` go out. A conflict therefore stops with the
//! repository as it was. Each branch is rebased with `--onto` from its old base,
//! so commits of a parent that was squashed in do not come along.

use crate::github::Pull;
use crate::gitops;
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::time::Duration;

const LOCAL: Duration = Duration::from_secs(30);
const NETWORK: Duration = Duration::from_secs(180);
const REBASE: Duration = Duration::from_secs(300);
/// How far a chain of merged parents is followed.
const MAX_MERGED_HOPS: usize = 16;

/// What a chain needs to know of an open pull or merge request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Change {
    pub number: i64,
    pub branch: String,
    pub base: String,
    /// From a fork: its branch is not ours to rebase.
    pub foreign: bool,
}

/// One that is merged: where its branch went.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Merged {
    pub number: i64,
    pub branch: String,
    pub base: String,
    /// The commit its branch was at.
    pub head: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Member {
    pub number: i64,
    pub depth: usize,
    pub parent: Option<i64>,
}

/// A chain: its root first, each child right after its parent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stack {
    pub root: i64,
    pub members: Vec<Member>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Detected {
    pub stacks: Vec<Stack>,
    /// Bases that lead in a circle: not a chain anyone can update.
    pub cycles: Vec<i64>,
}

/// Chains of at least two, from each change's base branch. A change whose base
/// is no other change's branch is a root; a parent with two children forks.
pub fn detect(changes: &[Change]) -> Detected {
    let mut own: Vec<&Change> = changes.iter().filter(|c| !c.foreign).collect();
    own.sort_by_key(|c| c.number);
    let mut by_branch: BTreeMap<&str, &Change> = BTreeMap::new();
    for c in &own {
        by_branch.entry(c.branch.as_str()).or_insert(c);
    }
    let parent_of = |c: &Change| by_branch.get(c.base.as_str()).filter(|p| p.number != c.number).map(|p| p.number);
    let mut children: BTreeMap<i64, Vec<i64>> = BTreeMap::new();
    for c in &own {
        if let Some(p) = parent_of(c) {
            children.entry(p).or_default().push(c.number);
        }
    }
    let mut seen = BTreeSet::new();
    let mut stacks = Vec::new();
    for c in own.iter().filter(|c| parent_of(c).is_none()) {
        let mut members = Vec::new();
        // Last in, first out: children reversed, so the lowest number comes next.
        let mut todo = vec![(c.number, 0usize, None)];
        while let Some((number, depth, parent)) = todo.pop() {
            if !seen.insert(number) {
                continue;
            }
            members.push(Member { number, depth, parent });
            for kid in children.get(&number).into_iter().flatten().rev() {
                todo.push((*kid, depth + 1, Some(number)));
            }
        }
        if members.len() >= 2 {
            stacks.push(Stack { root: c.number, members });
        }
    }
    let cycles = own.iter().map(|c| c.number).filter(|n| !seen.contains(n)).collect();
    Detected { stacks, cycles }
}

/// A row's place in its chain, for the list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StackPlace {
    pub root: i64,
    pub depth: usize,
    pub size: usize,
    pub first: bool,
}

pub fn change_of(p: &Pull) -> Change {
    Change { number: p.number, branch: p.branch.clone(), base: p.base.clone(), foreign: p.foreign }
}

/// The list with each chain's rows together and in order, at the place of its
/// first row; everything else keeps its place.
pub fn arrange(pulls: Vec<Pull>) -> Vec<Pull> {
    let changes: Vec<Change> = pulls.iter().map(change_of).collect();
    let detected = detect(&changes);
    let mut place: BTreeMap<i64, (i64, StackPlace)> = BTreeMap::new();
    for s in &detected.stacks {
        for (i, m) in s.members.iter().enumerate() {
            place.insert(m.number, (s.root, StackPlace { root: s.root, depth: m.depth, size: s.members.len(), first: i == 0 }));
        }
    }
    let mut rows: BTreeMap<i64, Pull> = pulls.iter().cloned().map(|p| (p.number, p)).collect();
    let mut out = Vec::with_capacity(pulls.len());
    for p in &pulls {
        let Some((root, _)) = place.get(&p.number) else {
            out.push(p.clone());
            continue;
        };
        if let Some(stack) = detected.stacks.iter().find(|s| s.root == *root) {
            for m in &stack.members {
                if let Some(mut row) = rows.remove(&m.number) {
                    row.stack = place.get(&m.number).map(|(_, at)| at.clone());
                    out.push(row);
                }
            }
        }
    }
    out
}

// ── The plan ──

/// One line of what will happen, shown before anything does.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Step {
    /// `rebase` | `push` | `retarget`
    pub kind: String,
    pub text: String,
    pub command: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Onto {
    /// A commit as it is now.
    Commit(String),
    /// What this change's rebase will produce.
    Rebased(i64),
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Rebase {
    number: i64,
    branch: String,
    old_head: String,
    old_base: String,
    onto: Onto,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Plan {
    /// Changes when anything would be done differently: what was confirmed is
    /// what runs.
    pub id: String,
    pub root: i64,
    pub steps: Vec<Step>,
    /// Why it cannot run, in words.
    pub blocked: Option<String>,
    #[serde(skip)]
    rebases: Vec<Rebase>,
    #[serde(skip)]
    retargets: Vec<(i64, String)>,
}

fn git(cwd: &Path, args: &[&str]) -> Result<String, String> {
    gitops::run("git", cwd, args, LOCAL)
}

fn tip(cwd: &Path, refname: &str) -> Option<String> {
    git(cwd, &["rev-parse", "--verify", "--quiet", &format!("{refname}^{{commit}}")]).ok().map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
}

fn is_ancestor(cwd: &Path, older: &str, newer: &str) -> bool {
    git(cwd, &["merge-base", "--is-ancestor", older, newer]).is_ok()
}

fn merge_base(cwd: &Path, a: &str, b: &str) -> Result<String, String> {
    git(cwd, &["merge-base", a, b]).map(|s| s.trim().to_string()).map_err(|_| format!("{} and {} share no history.", short(a), short(b)))
}

fn short(sha: &str) -> &str {
    sha.get(..8).unwrap_or(sha)
}

fn remote_ref(branch: &str) -> String {
    format!("refs/remotes/origin/{branch}")
}

/// Where the chain's first change goes: its base, or what a merged parent went
/// into. Returns the branch, the commit its own work starts after, and whether
/// the host still has it on another base.
fn root_target(cwd: &Path, c: &Change, head: &str, merged: &[Merged]) -> Result<(String, String, bool), String> {
    let mut target = c.base.clone();
    let mut old_base = None;
    let mut moved = false;
    for _ in 0..MAX_MERGED_HOPS {
        let Some(m) = merged.iter().find(|m| m.branch == target && m.number != c.number) else {
            break;
        };
        if old_base.is_none() && is_ancestor(cwd, &m.head, head) {
            old_base = Some(m.head.clone());
        }
        target = m.base.clone();
        moved = true;
    }
    let target_tip = tip(cwd, &remote_ref(&target)).ok_or_else(|| format!("{target} is not on origin."))?;
    if !moved {
        // The host moved it already when the parent's branch was deleted. A
        // parent merged with a commit is in the base's history and needs nothing
        // special; one that was squashed in is not, and must not be replayed.
        old_base = merged.iter().find(|m| m.base == target && m.number != c.number && is_ancestor(cwd, &m.head, head) && !is_ancestor(cwd, &m.head, &target_tip)).map(|m| m.head.clone());
    }
    let old_base = match old_base {
        Some(b) => b,
        None => merge_base(cwd, &target_tip, head)?,
    };
    Ok((target, old_base, moved))
}

/// Fetches, then works out what updating the chain takes. Nothing else changes.
/// `sign` is how the host writes a number: `#` or `!`.
pub fn build_plan_as(cwd: &Path, root: i64, changes: &[Change], merged: &[Merged], sign: &str, retarget_command: &dyn Fn(i64, &str) -> String) -> Result<Plan, String> {
    let detected = detect(changes);
    let stack = detected.stacks.iter().find(|s| s.root == root).ok_or("That chain is not there any more: refresh the list.")?;
    gitops::run("git", cwd, &["fetch", "origin"], NETWORK)?;
    let change = |n: i64| changes.iter().find(|c| c.number == n);
    let mut plan = Plan { root, ..Default::default() };
    let mut rebased = BTreeSet::new();
    for m in &stack.members {
        let c = change(m.number).ok_or("That chain changed: refresh the list.")?;
        let head = tip(cwd, &remote_ref(&c.branch)).ok_or_else(|| format!("{} is not on origin.", c.branch))?;
        if let Some(local) = tip(cwd, &format!("refs/heads/{}", c.branch)) {
            if local != head && !is_ancestor(cwd, &local, &head) {
                let mut blocked = Plan { root, blocked: Some(format!("Your local {} has commits that are not on origin. Push them or drop them first; nothing was changed.", c.branch)), ..Default::default() };
                blocked.id = plan_id(&blocked);
                return Ok(blocked);
            }
        }
        let (onto, label, old_base) = match m.parent.and_then(change) {
            Some(parent) => {
                let parent_tip = tip(cwd, &remote_ref(&parent.branch)).ok_or_else(|| format!("{} is not on origin.", parent.branch))?;
                let old_base = merge_base(cwd, &parent_tip, &head)?;
                let onto = if rebased.contains(&parent.number) { Onto::Rebased(parent.number) } else { Onto::Commit(parent_tip) };
                (onto, parent.branch.clone(), old_base)
            }
            None => {
                let (target, old_base, moved) = root_target(cwd, c, &head, merged)?;
                let target_tip = tip(cwd, &remote_ref(&target)).ok_or_else(|| format!("{target} is not on origin."))?;
                if moved {
                    plan.retargets.push((c.number, target.clone()));
                }
                (Onto::Commit(target_tip), target, old_base)
            }
        };
        let current = matches!(&onto, Onto::Commit(sha) if *sha == old_base);
        if !current {
            let shown = match &onto {
                Onto::Commit(sha) => short(sha).to_string(),
                Onto::Rebased(_) => format!("<new {label}>"),
            };
            plan.steps.push(Step {
                kind: "rebase".into(),
                text: format!("Rebase {} ({sign}{}) onto {label}", c.branch, c.number),
                command: format!("git rebase --onto {shown} {} {}", short(&old_base), short(&head)),
            });
            plan.rebases.push(Rebase { number: c.number, branch: c.branch.clone(), old_head: head, old_base, onto });
            rebased.insert(c.number);
        }
    }
    if !plan.rebases.is_empty() {
        let names: Vec<&str> = plan.rebases.iter().map(|r| r.branch.as_str()).collect();
        let leases: Vec<String> = plan.rebases.iter().map(|r| format!("--force-with-lease={}:{} <new {}>:refs/heads/{}", r.branch, short(&r.old_head), r.branch, r.branch)).collect();
        plan.steps.push(Step {
            kind: "push".into(),
            text: format!("Push {}: all or none, and only if nobody else pushed since", names.join(", ")),
            command: format!("git push --atomic origin {}", leases.join(" ")),
        });
    }
    for (number, target) in &plan.retargets {
        let base = change(*number).map(|c| c.base.clone()).unwrap_or_default();
        plan.steps.push(Step { kind: "retarget".into(), text: format!("Point {sign}{number} at {target} instead of {base}"), command: retarget_command(*number, target) });
    }
    plan.id = plan_id(&plan);
    Ok(plan)
}

fn plan_id(plan: &Plan) -> String {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    plan.root.hash(&mut h);
    plan.blocked.hash(&mut h);
    for r in &plan.rebases {
        (r.number, &r.old_head, &r.old_base).hash(&mut h);
        match &r.onto {
            Onto::Commit(sha) => sha.hash(&mut h),
            Onto::Rebased(n) => n.hash(&mut h),
        }
    }
    plan.retargets.hash(&mut h);
    format!("{:016x}", h.finish())
}

// ── Running it ──

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StepResult {
    pub text: String,
    /// `done` | `failed` | `skipped`, or `dropped`: it worked, but what came
    /// after failed, so its result was thrown away.
    pub status: String,
    pub detail: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Outcome {
    pub ok: bool,
    pub message: String,
    pub steps: Vec<StepResult>,
    /// The files a rebase could not bring together.
    pub conflict: Vec<String>,
    /// Checked-out branches left on their old commits, with what moves them.
    pub notes: Vec<String>,
}

/// A worktree of its own for the rebases; gone again whatever happens.
struct Scratch {
    repo: PathBuf,
    path: PathBuf,
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let path = self.path.to_string_lossy().into_owned();
        let _ = git(&self.repo, &["worktree", "remove", "--force", &path]);
        let _ = std::fs::remove_dir_all(&self.path);
        let _ = git(&self.repo, &["worktree", "prune"]);
    }
}

fn scratch(cwd: &Path, start: &str) -> Result<Scratch, String> {
    let path = std::env::temp_dir().join(format!("qs-stack-{}", uuid::Uuid::new_v4().simple()));
    let shown = path.to_string_lossy().into_owned();
    git(cwd, &["worktree", "add", "--detach", "--quiet", &shown, start]).map_err(|e| format!("could not make a place to rebase: {e}"))?;
    Ok(Scratch { repo: cwd.to_path_buf(), path })
}

/// Branches checked out in a worktree of this repository, and where.
fn checked_out(cwd: &Path) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    let mut at = String::new();
    for line in git(cwd, &["worktree", "list", "--porcelain"]).unwrap_or_default().lines() {
        if let Some(p) = line.strip_prefix("worktree ") {
            at = p.to_string();
        } else if let Some(b) = line.strip_prefix("branch refs/heads/") {
            out.insert(b.to_string(), at.clone());
        }
    }
    out
}

/// Stopped before anything left this machine: rebases that worked are not kept.
fn finish(plan: &Plan, results: Vec<StepResult>, message: String, conflict: Vec<String>) -> Outcome {
    let mut steps = results;
    for s in steps.iter_mut().filter(|s| s.status == "done") {
        s.status = "dropped".into();
    }
    for s in plan.steps.iter().skip(steps.len()) {
        steps.push(StepResult { text: s.text.clone(), status: "skipped".into(), detail: String::new() });
    }
    Outcome { ok: false, message, steps, conflict, notes: Vec::new() }
}

/// Runs a plan: the rebases aside, one atomic push, then what the host needs.
pub fn run_plan(cwd: &Path, plan: &Plan, retarget: &dyn Fn(i64, &str) -> Result<(), String>) -> Outcome {
    if let Some(why) = &plan.blocked {
        return Outcome { message: why.clone(), ..Default::default() };
    }
    let mut results = Vec::new();
    let mut new_tips: BTreeMap<i64, String> = BTreeMap::new();
    if let Some(first) = plan.rebases.first() {
        let tree = match scratch(cwd, &first.old_head) {
            Ok(t) => t,
            Err(e) => return finish(plan, results, e, Vec::new()),
        };
        for r in &plan.rebases {
            let step = &plan.steps[results.len()];
            let onto = match &r.onto {
                Onto::Commit(sha) => sha.clone(),
                Onto::Rebased(n) => new_tips.get(n).cloned().unwrap_or_default(),
            };
            let done = git(&tree.path, &["checkout", "--detach", "--quiet", &r.old_head]).and_then(|_| gitops::run("git", &tree.path, &["-c", "core.editor=true", "rebase", "--onto", &onto, &r.old_base], REBASE));
            if let Err(e) = done {
                let files: Vec<String> = git(&tree.path, &["diff", "--name-only", "--diff-filter=U"]).unwrap_or_default().lines().map(str::to_string).filter(|l| !l.is_empty()).collect();
                let _ = git(&tree.path, &["rebase", "--abort"]);
                results.push(StepResult { text: step.text.clone(), status: "failed".into(), detail: if files.is_empty() { e.clone() } else { format!("Conflict in {}", files.join(", ")) } });
                let message = if files.is_empty() {
                    format!("Rebasing {} failed, so nothing was changed: {e}", r.branch)
                } else {
                    format!("{} conflicts with what it goes onto, so nothing was changed. Fix it on that branch, then update again.", r.branch)
                };
                return finish(plan, results, message, files);
            }
            match tip(&tree.path, "HEAD") {
                Some(new) => {
                    results.push(StepResult { text: step.text.clone(), status: "done".into(), detail: format!("{} → {}", short(&r.old_head), short(&new)) });
                    new_tips.insert(r.number, new);
                }
                None => {
                    results.push(StepResult { text: step.text.clone(), status: "failed".into(), detail: "No commit came out of the rebase.".into() });
                    return finish(plan, results, format!("Rebasing {} left nothing to push, so nothing was changed.", r.branch), Vec::new());
                }
            }
        }
        let mut args: Vec<String> = vec!["push".into(), "--atomic".into(), "origin".into()];
        for r in &plan.rebases {
            args.push(format!("--force-with-lease={}:{}", r.branch, r.old_head));
            args.push(format!("{}:refs/heads/{}", new_tips[&r.number], r.branch));
        }
        let argv: Vec<&str> = args.iter().map(String::as_str).collect();
        let step = &plan.steps[results.len()];
        if let Err(e) = gitops::run("git", cwd, &argv, NETWORK) {
            results.push(StepResult { text: step.text.clone(), status: "failed".into(), detail: e });
            return finish(plan, results, "The push was refused, so no branch changed: someone may have pushed in the meantime. Look at the plan again.".into(), Vec::new());
        }
        results.push(StepResult { text: step.text.clone(), status: "done".into(), detail: String::new() });
    }
    // The branches here follow, unless one is checked out: its files are the user's.
    let mut notes = Vec::new();
    let open = checked_out(cwd);
    for r in &plan.rebases {
        let local = format!("refs/heads/{}", r.branch);
        let Some(old) = tip(cwd, &local) else {
            continue;
        };
        if let Some(at) = open.get(&r.branch) {
            notes.push(format!("{} is checked out in {at} and still has its old commits. When that checkout is clean, run git reset --hard origin/{} there.", r.branch, r.branch));
        } else if let Err(e) = git(cwd, &["update-ref", &local, &new_tips[&r.number], &old]) {
            notes.push(format!("Your local {} did not move: {e}", r.branch));
        }
    }
    let mut ok = true;
    for (i, (number, target)) in plan.retargets.iter().enumerate() {
        let step = &plan.steps[plan.rebases.len() + usize::from(!plan.rebases.is_empty()) + i];
        match retarget(*number, target) {
            Ok(()) => results.push(StepResult { text: step.text.clone(), status: "done".into(), detail: String::new() }),
            Err(e) => {
                ok = false;
                results.push(StepResult { text: step.text.clone(), status: "failed".into(), detail: e });
            }
        }
    }
    let message = if ok {
        format!("Updated the chain: {} rebased.", plan.rebases.len())
    } else {
        "The branches are updated, but a base could not be changed. Change it on the host, or update again.".to_string()
    };
    Outcome { ok, message, steps: results, conflict: Vec::new(), notes }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn build_plan(cwd: &Path, root: i64, changes: &[Change], merged: &[Merged], retarget_command: &dyn Fn(i64, &str) -> String) -> Result<Plan, String> {
        build_plan_as(cwd, root, changes, merged, "#", retarget_command)
    }

    fn change(number: i64, branch: &str, base: &str) -> Change {
        Change { number, branch: branch.into(), base: base.into(), foreign: false }
    }
    fn numbers(s: &Stack) -> Vec<(i64, usize)> {
        s.members.iter().map(|m| (m.number, m.depth)).collect()
    }

    #[test]
    fn a_line_of_three_is_one_chain_in_order() {
        // Listed newest first, as hosts list them.
        let d = detect(&[change(3, "c", "b"), change(2, "b", "a"), change(1, "a", "main")]);
        assert_eq!(d.stacks.len(), 1);
        assert_eq!(numbers(&d.stacks[0]), vec![(1, 0), (2, 1), (3, 2)]);
        assert_eq!(d.stacks[0].members[1].parent, Some(1));
        assert!(d.cycles.is_empty());
    }

    #[test]
    fn a_fork_keeps_each_child_after_its_parent() {
        let d = detect(&[change(1, "a", "main"), change(2, "b", "a"), change(3, "c", "a"), change(4, "d", "b")]);
        assert_eq!(numbers(&d.stacks[0]), vec![(1, 0), (2, 1), (4, 2), (3, 1)]);
    }

    #[test]
    fn a_cycle_is_not_a_chain() {
        let d = detect(&[change(1, "a", "b"), change(2, "b", "a"), change(3, "c", "main"), change(4, "d", "c")]);
        assert_eq!(d.cycles, vec![1, 2]);
        assert_eq!(d.stacks.len(), 1);
        assert_eq!(d.stacks[0].root, 3);
        // A change whose base is its own branch only leads to itself: nothing to chain.
        let own = detect(&[change(5, "e", "e")]);
        assert!(own.stacks.is_empty());
    }

    #[test]
    fn single_changes_and_forks_of_other_repositories_are_no_chain() {
        let d = detect(&[change(1, "a", "main"), change(2, "b", "main")]);
        assert!(d.stacks.is_empty());
        assert!(d.cycles.is_empty());
        // "a" from someone's fork is not our branch "a": neither parent nor child.
        let foreign = Change { foreign: true, ..change(3, "a", "main") };
        let d = detect(&[foreign, change(4, "x", "a")]);
        assert!(d.stacks.is_empty());
    }

    fn pull(number: i64, branch: &str, base: &str) -> Pull {
        Pull {
            number,
            title: format!("change {number}"),
            url: String::new(),
            state: "open".into(),
            author: String::new(),
            branch: branch.into(),
            base: base.into(),
            head_oid: String::new(),
            foreign: false,
            review: String::new(),
            checks: Default::default(),
            additions: 0,
            deletions: 0,
            labels: Vec::new(),
            updated_at: String::new(),
            ready: false,
            note: String::new(),
            stack: None,
        }
    }

    #[test]
    fn the_list_keeps_a_chain_together_and_everything_else_in_place() {
        let rows = arrange(vec![pull(9, "other", "main"), pull(3, "c", "b"), pull(8, "more", "main"), pull(1, "a", "main"), pull(2, "b", "a")]);
        assert_eq!(rows.iter().map(|p| p.number).collect::<Vec<_>>(), vec![9, 1, 2, 3, 8]);
        assert_eq!(rows[1].stack, Some(StackPlace { root: 1, depth: 0, size: 3, first: true }));
        assert_eq!(rows[3].stack, Some(StackPlace { root: 1, depth: 2, size: 3, first: false }));
        assert_eq!(rows[0].stack, None);
    }

    // ── Real repositories: a bare origin and a clone ──

    struct Repos {
        dir: PathBuf,
        work: PathBuf,
        origin: PathBuf,
    }

    impl Drop for Repos {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.dir);
        }
    }

    fn g(cwd: &Path, args: &[&str]) -> String {
        let out = std::process::Command::new("git").args(args).current_dir(cwd).output().expect("git");
        assert!(out.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&out.stderr));
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    }

    fn commit(cwd: &Path, file: &str, text: &str, message: &str) -> String {
        fs::write(cwd.join(file), text).unwrap();
        g(cwd, &["add", "-A"]);
        g(cwd, &["commit", "-qm", message]);
        g(cwd, &["rev-parse", "HEAD"])
    }

    /// main ← a ← b ← c, all pushed, with `main` checked out.
    fn repos() -> Repos {
        let dir = std::env::temp_dir().join(format!("qs-stacks-{}", uuid::Uuid::new_v4().simple()));
        let origin = dir.join("origin.git");
        let work = dir.join("work");
        fs::create_dir_all(&origin).unwrap();
        fs::create_dir_all(&work).unwrap();
        g(&origin, &["init", "--bare", "-q", "-b", "main"]);
        g(&work, &["init", "-q", "-b", "main"]);
        for (k, v) in [("user.name", "QA"), ("user.email", "qa@example.test"), ("commit.gpgsign", "false"), ("core.autocrlf", "false")] {
            g(&work, &["config", k, v]);
        }
        g(&work, &["remote", "add", "origin", &origin.to_string_lossy()]);
        commit(&work, "base.txt", "one\ntwo\nthree\n", "base");
        g(&work, &["push", "-q", "-u", "origin", "main"]);
        let mut parent = "main";
        for name in ["a", "b", "c"] {
            g(&work, &["switch", "-q", "-c", name, parent]);
            commit(&work, &format!("{name}.txt"), name, &format!("work on {name}"));
            g(&work, &["push", "-q", "-u", "origin", name]);
            parent = name;
        }
        g(&work, &["switch", "-q", "main"]);
        Repos { dir, work, origin }
    }

    fn chain() -> Vec<Change> {
        vec![change(1, "a", "main"), change(2, "b", "a"), change(3, "c", "b")]
    }

    fn no_host(_: i64, _: &str) -> String {
        "host".into()
    }

    fn refs(cwd: &Path) -> String {
        g(cwd, &["for-each-ref", "--format=%(refname) %(objectname)"])
    }

    /// A commit on main from elsewhere: the base moved.
    fn advance_main(r: &Repos, file: &str, text: &str) {
        g(&r.work, &["switch", "-q", "main"]);
        commit(&r.work, file, text, "main moved");
        g(&r.work, &["push", "-q", "origin", "main"]);
    }

    /// What is on a branch of origin that is not on main, oldest first.
    fn subjects(r: &Repos, branch: &str, since: &str) -> Vec<String> {
        g(&r.origin, &["log", "--reverse", "--format=%s", &format!("{since}..{branch}")]).lines().map(str::to_string).collect()
    }

    #[test]
    fn the_steps_write_numbers_the_way_the_host_does() {
        let r = repos();
        advance_main(&r, "main.txt", "new");
        let plan = build_plan_as(&r.work, 1, &chain(), &[], "!", &no_host).unwrap();
        assert_eq!(plan.steps[0].text, "Rebase a (!1) onto main");
    }

    #[test]
    fn an_up_to_date_chain_has_nothing_to_do() {
        let r = repos();
        let plan = build_plan(&r.work, 1, &chain(), &[], &no_host).unwrap();
        assert_eq!(plan.steps, vec![]);
        assert_eq!(plan.blocked, None);
    }

    #[test]
    fn when_the_base_moves_the_whole_chain_follows_in_one_push() {
        let r = repos();
        advance_main(&r, "main.txt", "new");
        let plan = build_plan(&r.work, 1, &chain(), &[], &no_host).unwrap();
        let kinds: Vec<&str> = plan.steps.iter().map(|s| s.kind.as_str()).collect();
        assert_eq!(kinds, vec!["rebase", "rebase", "rebase", "push"]);
        assert_eq!(plan.steps[1].text, "Rebase b (#2) onto a");
        assert!(plan.steps[3].command.contains("--force-with-lease=b:"), "{}", plan.steps[3].command);

        let out = run_plan(&r.work, &plan, &|_, _| panic!("no retarget here"));
        assert!(out.ok, "{out:?}");
        for (branch, want) in [("a", vec!["work on a"]), ("b", vec!["work on a", "work on b"]), ("c", vec!["work on a", "work on b", "work on c"])] {
            assert_eq!(subjects(&r, branch, "main"), want, "{branch}");
            assert!(is_ancestor(&r.origin, "main", branch), "{branch} sits on the new main");
            // The branches here moved too, as none is checked out.
            assert_eq!(g(&r.work, &["rev-parse", branch]), g(&r.origin, &["rev-parse", branch]));
        }
        assert_eq!(g(&r.work, &["worktree", "list", "--porcelain"]).matches("worktree ").count(), 1, "the place to rebase is gone");
        // Done means done: looking again finds nothing.
        assert_eq!(build_plan(&r.work, 1, &chain(), &[], &no_host).unwrap().steps, vec![]);
    }

    #[test]
    fn after_a_squash_merge_the_rest_goes_onto_the_base_without_the_parents_commits() {
        let r = repos();
        let a_head = g(&r.origin, &["rev-parse", "a"]);
        // a is squashed into main on the host.
        g(&r.work, &["switch", "-q", "main"]);
        g(&r.work, &["merge", "-q", "--squash", "a"]);
        g(&r.work, &["commit", "-qm", "a, squashed"]);
        g(&r.work, &["push", "-q", "origin", "main"]);
        let open = vec![change(2, "b", "a"), change(3, "c", "b")];
        let merged = vec![Merged { number: 1, branch: "a".into(), base: "main".into(), head: a_head }];
        let plan = build_plan(&r.work, 2, &open, &merged, &|n, b| format!("retarget {n} {b}")).unwrap();
        let kinds: Vec<&str> = plan.steps.iter().map(|s| s.kind.as_str()).collect();
        assert_eq!(kinds, vec!["rebase", "rebase", "push", "retarget"]);
        assert_eq!(plan.steps[3].text, "Point #2 at main instead of a");
        assert_eq!(plan.steps[3].command, "retarget 2 main");

        let moved = std::cell::RefCell::new(Vec::new());
        let out = run_plan(&r.work, &plan, &|n, b| {
            moved.borrow_mut().push((n, b.to_string()));
            Ok(())
        });
        assert!(out.ok, "{out:?}");
        assert_eq!(*moved.borrow(), vec![(2, "main".to_string())]);
        assert_eq!(subjects(&r, "b", "main"), vec!["work on b"], "a's own commit did not come along");
        assert_eq!(subjects(&r, "c", "main"), vec!["work on b", "work on c"]);
    }

    #[test]
    fn a_parent_the_host_already_moved_past_is_still_left_out() {
        let r = repos();
        let a_head = g(&r.origin, &["rev-parse", "a"]);
        g(&r.work, &["switch", "-q", "main"]);
        g(&r.work, &["merge", "-q", "--squash", "a"]);
        g(&r.work, &["commit", "-qm", "a, squashed"]);
        g(&r.work, &["push", "-q", "origin", "main"]);
        // The host retargeted b when a's branch was deleted.
        let open = vec![change(2, "b", "main"), change(3, "c", "b")];
        let merged = vec![Merged { number: 1, branch: "a".into(), base: "main".into(), head: a_head }];
        let plan = build_plan(&r.work, 2, &open, &merged, &no_host).unwrap();
        assert!(plan.steps.iter().all(|s| s.kind != "retarget"));
        let out = run_plan(&r.work, &plan, &|_, _| panic!("already on main"));
        assert!(out.ok, "{out:?}");
        assert_eq!(subjects(&r, "b", "main"), vec!["work on b"]);
    }

    #[test]
    fn a_conflict_stops_and_leaves_the_repository_as_it_was() {
        let r = repos();
        // b changes a file; main changes the same line.
        g(&r.work, &["switch", "-q", "b"]);
        commit(&r.work, "base.txt", "one\nfrom b\nthree\n", "b edits base");
        g(&r.work, &["push", "-q", "origin", "b"]);
        g(&r.work, &["switch", "-q", "c"]);
        g(&r.work, &["merge", "-q", "--no-edit", "b"]);
        g(&r.work, &["push", "-q", "origin", "c"]);
        advance_main(&r, "base.txt", "one\nfrom main\nthree\n");
        // Uncommitted work in the checkout, which must not notice any of this.
        fs::write(r.work.join("mine.txt"), "not committed").unwrap();
        let heads = |cwd: &Path| g(cwd, &["for-each-ref", "--format=%(refname) %(objectname)", "refs/heads"]);
        let (heads_before, origin_before, status_before) = (heads(&r.work), refs(&r.origin), g(&r.work, &["status", "--porcelain"]));

        let plan = build_plan(&r.work, 1, &chain(), &[], &no_host).unwrap();
        // Building the plan fetched: no branch moved for it.
        assert_eq!(heads(&r.work), heads_before);
        let refs_before = refs(&r.work);
        let out = run_plan(&r.work, &plan, &|_, _| panic!("never gets here"));
        assert!(!out.ok);
        assert_eq!(out.conflict, vec!["base.txt"]);
        assert!(out.message.contains("nothing was changed"), "{}", out.message);
        assert_eq!(out.steps.iter().map(|s| s.status.as_str()).collect::<Vec<_>>(), vec!["dropped", "failed", "skipped", "skipped"]);

        assert_eq!(refs(&r.work), refs_before, "every ref here is where it was");
        assert_eq!(refs(&r.origin), origin_before, "nothing reached origin");
        assert_eq!(g(&r.work, &["status", "--porcelain"]), status_before);
        assert_eq!(fs::read_to_string(r.work.join("mine.txt")).unwrap(), "not committed");
        assert_eq!(g(&r.work, &["branch", "--show-current"]), "main");
        assert_eq!(g(&r.work, &["worktree", "list", "--porcelain"]).matches("worktree ").count(), 1);
    }

    #[test]
    fn someone_elses_push_refuses_all_of_it() {
        let r = repos();
        advance_main(&r, "main.txt", "new");
        let plan = build_plan(&r.work, 1, &chain(), &[], &no_host).unwrap();
        // After the plan, another clone pushes to b.
        let other = r.dir.join("other");
        g(&r.dir, &["clone", "-q", "-b", "b", &r.origin.to_string_lossy(), "other"]);
        g(&other, &["config", "user.name", "Other"]);
        g(&other, &["config", "user.email", "other@example.test"]);
        commit(&other, "theirs.txt", "theirs", "their work");
        g(&other, &["push", "-q", "origin", "b"]);
        let (origin_before, refs_before) = (refs(&r.origin), refs(&r.work));

        let out = run_plan(&r.work, &plan, &|_, _| panic!("never gets here"));
        assert!(!out.ok);
        assert!(out.message.contains("no branch changed"), "{}", out.message);
        assert_eq!(out.steps.iter().map(|s| s.status.as_str()).collect::<Vec<_>>(), vec!["dropped", "dropped", "dropped", "failed"]);
        assert_eq!(refs(&r.origin), origin_before, "atomic: a and c did not go either");
        assert_eq!(refs(&r.work), refs_before);
    }

    #[test]
    fn local_commits_that_are_not_pushed_block_it() {
        let r = repos();
        g(&r.work, &["switch", "-q", "b"]);
        commit(&r.work, "local.txt", "only here", "local work");
        g(&r.work, &["switch", "-q", "main"]);
        advance_main(&r, "main.txt", "new");
        let plan = build_plan(&r.work, 1, &chain(), &[], &no_host).unwrap();
        assert!(plan.blocked.as_deref().unwrap_or_default().contains("Your local b has commits"), "{plan:?}");
        assert_eq!(plan.steps, vec![]);
        let before = refs(&r.work);
        let out = run_plan(&r.work, &plan, &|_, _| panic!("blocked"));
        assert!(!out.ok);
        assert_eq!(refs(&r.work), before);
    }

    #[test]
    fn a_checked_out_branch_stays_and_the_result_says_so() {
        let r = repos();
        advance_main(&r, "main.txt", "new");
        g(&r.work, &["switch", "-q", "b"]);
        let here = g(&r.work, &["rev-parse", "HEAD"]);
        let plan = build_plan(&r.work, 1, &chain(), &[], &no_host).unwrap();
        let out = run_plan(&r.work, &plan, &|_, _| Ok(()));
        assert!(out.ok, "{out:?}");
        assert_eq!(g(&r.work, &["rev-parse", "HEAD"]), here, "its files are the user's");
        assert_eq!(out.notes.len(), 1);
        assert!(out.notes[0].starts_with("b is checked out in "), "{}", out.notes[0]);
        assert!(out.notes[0].contains("git reset --hard origin/b"), "{}", out.notes[0]);
    }

    #[test]
    fn a_host_that_refuses_the_new_base_is_reported_and_the_branches_stay_updated() {
        let r = repos();
        let a_head = g(&r.origin, &["rev-parse", "a"]);
        g(&r.work, &["switch", "-q", "main"]);
        g(&r.work, &["merge", "-q", "--squash", "a"]);
        g(&r.work, &["commit", "-qm", "a, squashed"]);
        g(&r.work, &["push", "-q", "origin", "main"]);
        let open = vec![change(2, "b", "a"), change(3, "c", "b")];
        let merged = vec![Merged { number: 1, branch: "a".into(), base: "main".into(), head: a_head }];
        let plan = build_plan(&r.work, 2, &open, &merged, &no_host).unwrap();
        let out = run_plan(&r.work, &plan, &|_, _| Err("refused".to_string()));
        assert!(!out.ok);
        assert_eq!(out.steps.last().unwrap().status, "failed");
        assert_eq!(out.steps.last().unwrap().detail, "refused");
        assert_eq!(subjects(&r, "b", "main"), vec!["work on b"]);
    }

    #[test]
    fn the_plan_changes_its_id_when_anything_moved() {
        let r = repos();
        advance_main(&r, "main.txt", "new");
        let first = build_plan(&r.work, 1, &chain(), &[], &no_host).unwrap();
        let again = build_plan(&r.work, 1, &chain(), &[], &no_host).unwrap();
        assert_eq!(first.id, again.id);
        advance_main(&r, "more.txt", "more");
        assert_ne!(first.id, build_plan(&r.work, 1, &chain(), &[], &no_host).unwrap().id);
    }
}
