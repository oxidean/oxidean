//! Binary-safe Myers O(ND) line diff (u4, shared with u5 merge).
//!
//! [`diff_lines`] compares byte-slice lines — NUL bytes, CRLF terminators,
//! and arbitrary binary content compare verbatim (line *content* equality,
//! not `\n` splitting, is the caller's contract). The divide-and-conquer
//! middle-snake recursion keeps worst-case memory at O(N+M) working vectors
//! plus O(log D) recursion depth.
//!
//! [`LINE_CAP`] is the whole-range-replacement tripwire: when the combined
//! input exceeds it the diff degenerates to one hunk — call sites that
//! page/blame through enormous blobs hit the cap instead of a quadratic
//! crawl (SR4.8 honest-bounds register).

/// Combined line-count ceiling — past this, return one whole-range
/// replacement hunk instead of running Myers (SR4.8).
pub const LINE_CAP: usize = 200_000;

/// A maximal run of non-equal lines in Myers order. Starts/lengths are
/// 0-based line indices into `old`/`new` respectively; a pure insertion has
/// `old_len == 0`, a pure deletion `new_len == 0`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LineHunk {
    pub old_start: usize,
    pub old_len: usize,
    pub new_start: usize,
    pub new_len: usize,
}

/// Diff two line vectors. Lines compare by exact byte equality — the caller
/// owns newline handling (a `b"\n"`-split, CRC32-preserving or not).
pub fn diff_lines(old: &[&[u8]], new: &[&[u8]]) -> Vec<LineHunk> {
    if old.len() + new.len() > LINE_CAP {
        // Coarse fallback: one whole-range replacement (never recurse into a
        // quadratic worst case).
        return vec![LineHunk {
            old_start: 0,
            old_len: old.len(),
            new_start: 0,
            new_len: new.len(),
        }];
    }
    let mut ops = Vec::new();
    conquer(old, new, &mut ops);
    hunks_from_ops(&ops)
}

#[derive(Debug, Clone, Copy)]
enum Op {
    Keep(usize),
    Delete(usize),
    Insert(usize),
}

/// Coalesce the ordered op stream into hunks: maximal adjacent
/// delete/insert runs, with positions reconstructed from the keep stream.
fn hunks_from_ops(ops: &[Op]) -> Vec<LineHunk> {
    let mut hunks = Vec::new();
    let (mut old_pos, mut new_pos) = (0usize, 0usize);
    let mut open: Option<LineHunk> = None;
    for op in ops {
        match *op {
            Op::Keep(n) => {
                if let Some(h) = open.take() {
                    hunks.push(h);
                }
                old_pos += n;
                new_pos += n;
            }
            Op::Delete(n) => {
                let h = open.get_or_insert(LineHunk {
                    old_start: old_pos,
                    old_len: 0,
                    new_start: new_pos,
                    new_len: 0,
                });
                h.old_len += n;
                old_pos += n;
            }
            Op::Insert(n) => {
                let h = open.get_or_insert(LineHunk {
                    old_start: old_pos,
                    old_len: 0,
                    new_start: new_pos,
                    new_len: 0,
                });
                h.new_len += n;
                new_pos += n;
            }
        }
    }
    if let Some(h) = open.take() {
        hunks.push(h);
    }
    hunks
}

fn common_prefix_len(a: &[&[u8]], b: &[&[u8]]) -> usize {
    let mut i = 0;
    while i < a.len() && i < b.len() && a[i] == b[i] {
        i += 1;
    }
    i
}

fn common_suffix_len(a: &[&[u8]], b: &[&[u8]]) -> usize {
    let mut i = 0;
    while i < a.len() && i < b.len() && a[a.len() - 1 - i] == b[b.len() - 1 - i] {
        i += 1;
    }
    i
}

/// Furthest-reaching D-path vectors indexed by diagonal `k` via
/// `offset + k` (`k ∈ [-max_d, max_d]`, usize slots).
struct V {
    offset: isize,
    v: Vec<usize>,
}

impl V {
    fn new(max_d: usize) -> Self {
        Self {
            offset: max_d as isize,
            v: vec![0; 2 * max_d.max(1)],
        }
    }
    fn get(&self, k: isize) -> usize {
        self.v[(k + self.offset) as usize]
    }
    fn set(&mut self, k: isize, x: usize) {
        self.v[(k + self.offset) as usize] = x;
    }
}

/// Myers middle snake: returns the split point `(x, y)` on `a`/`b` dividing
/// the edit script at the middle snake (classic D&C).
fn middle_snake(a: &[&[u8]], b: &[&[u8]]) -> (usize, usize) {
    let n = a.len();
    let m = b.len();
    let delta = n as isize - m as isize;
    let odd = delta % 2 != 0;
    let d_max = (n + m).div_ceil(2) + 1;
    let mut vf = V::new(d_max);
    let mut vb = V::new(d_max);
    vf.set(1, 0);
    vb.set(1, 0);

    for d in 0..d_max as isize {
        // Forward sweep.
        let mut k = d;
        while k >= -d {
            let mut x = if k == -d || (k != d && vf.get(k - 1) < vf.get(k + 1)) {
                vf.get(k + 1)
            } else {
                vf.get(k - 1) + 1
            };
            let y = (x as isize - k) as usize;
            let (x0, y0) = (x, y);
            if x < n && y < m {
                let snake = common_prefix_len(&a[x..], &b[y..]);
                x += snake;
            }
            vf.set(k, x);
            if odd && (k - delta).abs() < d && vf.get(k) + vb.get(delta - k) >= n {
                return (x0, y0);
            }
            k -= 2;
        }
        // Backward sweep — x/y count reversed-array steps from (n, m).
        let mut k = d;
        while k >= -d {
            let mut x = if k == -d || (k != d && vb.get(k - 1) < vb.get(k + 1)) {
                vb.get(k + 1)
            } else {
                vb.get(k - 1) + 1
            };
            let mut y = (x as isize - k) as usize;
            if x < n && y < m {
                // Reverse positions: compare a[n-1-x], b[m-1-y] going back.
                let snake = common_suffix_len(&a[..n - x], &b[..m - y]);
                x += snake;
                y += snake;
            }
            vb.set(k, x);
            if !odd && (k - delta).abs() <= d && vb.get(k) + vf.get(delta - k) >= n {
                return (n - x, m - y);
            }
            k -= 2;
        }
    }
    // Myers guarantees a return before exhausting d_max.
    unreachable!("middle snake not found")
}

/// Emit a minimal op stream covering `a` → `b` via divide & conquer.
fn conquer(a: &[&[u8]], b: &[&[u8]], ops: &mut Vec<Op>) {
    // Strip common prefix/suffix — emits Keeps and shrinks the problem.
    let prefix = common_prefix_len(a, b);
    if prefix > 0 {
        ops.push(Op::Keep(prefix));
    }
    let a = &a[prefix..];
    let b = &b[prefix..];
    let suffix = common_suffix_len(a, b);
    let a = &a[..a.len() - suffix];
    let b = &b[..b.len() - suffix];

    if !a.is_empty() && b.is_empty() {
        ops.push(Op::Delete(a.len()));
    } else if a.is_empty() && !b.is_empty() {
        ops.push(Op::Insert(b.len()));
    } else if !a.is_empty() && !b.is_empty() {
        let (x, y) = middle_snake(a, b);
        if (x == 0 && y == 0) || (x == a.len() && y == b.len()) {
            // Defensive: a corner split can't shrink the problem — emit the
            // whole-range replace rather than recurse forever.
            ops.push(Op::Delete(a.len()));
            ops.push(Op::Insert(b.len()));
        } else {
            conquer(&a[..x], &b[..y], ops);
            conquer(&a[x..], &b[y..], ops);
        }
    }
    if suffix > 0 {
        ops.push(Op::Keep(suffix));
    }
}

#[cfg(all(test, feature = "server"))]
mod tests {
    use super::*;

    fn lines<'a>(v: &'a [&'a str]) -> Vec<&'a [u8]> {
        v.iter().map(|s| s.as_bytes()).collect()
    }

    /// Rebuild `old` → `new` by applying hunks — every diff must transform
    /// correctly regardless of minimality.
    fn apply(old: &[&[u8]], new: &[&[u8]], hunks: &[LineHunk]) {
        let mut rebuilt: Vec<&[u8]> = Vec::new();
        let mut pos = 0usize;
        for h in hunks {
            assert!(h.old_start >= pos, "hunks must be ordered");
            rebuilt.extend_from_slice(&old[pos..h.old_start]);
            rebuilt.extend_from_slice(&new[h.new_start..h.new_start + h.new_len]);
            pos = h.old_start + h.old_len;
        }
        rebuilt.extend_from_slice(&old[pos..]);
        assert_eq!(rebuilt, new, "hunks must transform old → new");
    }

    #[test]
    fn empty_to_full_and_back() {
        let a = lines(&[]);
        let b = lines(&["l1", "l2", "l3"]);
        let h = diff_lines(&a, &b);
        assert_eq!(
            h,
            vec![LineHunk {
                old_start: 0,
                old_len: 0,
                new_start: 0,
                new_len: 3
            }]
        );
        apply(&a, &b, &h);
        let h = diff_lines(&b, &a);
        assert_eq!(
            h,
            vec![LineHunk {
                old_start: 0,
                old_len: 3,
                new_start: 0,
                new_len: 0
            }]
        );
        apply(&b, &a, &h);
    }

    #[test]
    fn identical_is_empty_diff() {
        let a = lines(&["x", "y", "z"]);
        assert_eq!(diff_lines(&a, &a), Vec::<LineHunk>::new());
        let both_empty: Vec<&[u8]> = vec![];
        assert_eq!(diff_lines(&both_empty, &both_empty), Vec::<LineHunk>::new());
    }

    #[test]
    fn prepend_append_middle() {
        let old = lines(&["b", "d", "e"]);
        let new = lines(&["a", "b", "c", "d", "e", "f"]);
        let h = diff_lines(&old, &new);
        apply(&old, &new, &h);
        // Minimal: "a" prepend, "c" insert between b/d, "f" append.
        assert_eq!(h.len(), 3);
        assert_eq!(h[0].old_start, 0);
        assert_eq!(h[0].old_len, 0);
        assert_eq!(h[0].new_len, 1);
        // Middle edit on shared content.
        let old = lines(&["a", "b", "c"]);
        let new = lines(&["a", "x", "y", "c"]);
        let h = diff_lines(&old, &new);
        apply(&old, &new, &h);
        assert_eq!(h.len(), 1);
        assert_eq!(h[0].old_start, 1);
        assert_eq!(h[0].old_len, 1);
        assert_eq!(h[0].new_start, 1);
        assert_eq!(h[0].new_len, 2);
    }

    #[test]
    fn shifted_block_is_move_not_rewrite() {
        let old = lines(&["a", "b", "c", "d", "e", "f"]);
        // Move "b","c" to the end.
        let new = lines(&["a", "d", "e", "f", "b", "c"]);
        let h = diff_lines(&old, &new);
        apply(&old, &new, &h);
        // Myers minimal edit: delete b,c / insert b,c — two small hunks or
        // one composed hunk, but never a whole-range replacement.
        let changed_old: usize = h.iter().map(|x| x.old_len).sum();
        let changed_new: usize = h.iter().map(|x| x.new_len).sum();
        assert!(
            changed_old <= 2 && changed_new <= 2,
            "non-minimal diff: {h:?}"
        );
    }

    #[test]
    fn binary_safe_nul_and_crlf() {
        let l1: &[u8] = b"line\x00one";
        let l2: &[u8] = b"line\x00two";
        let l3: &[u8] = b"crlf\r\nline";
        let old: Vec<&[u8]> = vec![l1, l3];
        let new: Vec<&[u8]> = vec![l2, l3];
        let h = diff_lines(&old, &new);
        apply(&old, &new, &h);
        assert_eq!(h.len(), 1);
        assert_eq!(h[0].old_len, 1);
        assert_eq!(h[0].new_len, 1);
        // Lines containing '\n' bytes still compare verbatim as units.
        let weird: &[u8] = b"a\nb";
        let old: Vec<&[u8]> = vec![weird];
        let new: Vec<&[u8]> = vec![weird];
        assert_eq!(diff_lines(&old, &new), Vec::<LineHunk>::new());
    }

    #[test]
    fn repeated_lines_minimal() {
        // Ambiguous duplicates still produce a valid transform.
        let old = lines(&["a", "a", "a", "b"]);
        let new = lines(&["a", "b", "a", "a"]);
        let h = diff_lines(&old, &new);
        apply(&old, &new, &h);
        let old_total: usize = h.iter().map(|x| x.old_len).sum();
        assert!(old_total <= 4);
    }

    #[test]
    fn cap_fallback_single_hunk() {
        let mut big: Vec<&[u8]> = Vec::new();
        let mut owned: Vec<Vec<u8>> = Vec::new();
        for i in 0..LINE_CAP + 1 {
            owned.push(format!("line{i}").into_bytes());
        }
        for l in &owned {
            big.push(l.as_slice());
        }
        let empty: Vec<&[u8]> = vec![];
        // old+new = LINE_CAP+1 > LINE_CAP → coarse whole-range fallback.
        let h = diff_lines(&big, &empty);
        assert_eq!(
            h,
            vec![LineHunk {
                old_start: 0,
                old_len: LINE_CAP + 1,
                new_start: 0,
                new_len: 0
            }]
        );
        // Under the cap the real algorithm runs.
        let under = &big[..LINE_CAP];
        let h = diff_lines(under, &empty);
        assert_eq!(h.len(), 1);
        assert_eq!(h[0].old_len, LINE_CAP);
    }

    #[test]
    fn mixed_insert_delete_same_region() {
        let old = lines(&["a", "b", "c", "d"]);
        let new = lines(&["a", "x", "y", "d"]);
        let h = diff_lines(&old, &new);
        apply(&old, &new, &h);
        // Adjacent del+ins coalesce to one replace hunk.
        assert_eq!(h.len(), 1);
        assert_eq!((h[0].old_start, h[0].old_len), (1, 2));
        assert_eq!((h[0].new_start, h[0].new_len), (1, 2));
    }
}
