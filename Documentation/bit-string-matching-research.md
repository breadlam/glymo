# Bit-string nearest-neighbor matching — research survey

*glymo / ghost-in-the-sshell — 2026-05-29.*

## Problem

glymo stores each glyph as a 32-bit bitmap (4×8 sub-pixels). The matcher
brute-force scans the pool with a popcount-class prune (`matcher.rs:87`).
Two upcoming phases share the same primitive — Hamming-nearest on
`u32`:

1. **Match** — per cell, per session, ~10⁸ calls/s at full scale.
2. **Dedup** — render every visible Unicode codepoint, collapse signature
   collisions, keep the lowest-codepoint representative. One-shot.

Parameters: `b = 32` (fixed). `N`: ~250 today → ≤ ~10⁶ post-Unicode-
dedup. Query radius `r`: low (noise flips a few bits). Metric: Hamming
(true metric — triangle inequality holds). Universe: 2³² ≈ 4.3 × 10⁹.

---

## Algorithms (chronological)

### SIMD popcount scan — baseline (`popcnt` 2007–08, AVX-512 `VPOPCNTQ` 2019)

`popcount(query ^ cand)` per candidate, vectorized over candidates.
Mula/Kurz/Lemire [AVX2 Harley-Seal (2016)](https://arxiv.org/pdf/1611.07612)
beats hardware `popcnt` for bulk; AVX-512 hits ~50 GB/s. **Beats every
index for small N** because lookup overhead dwarfs the 2-instruction
inner loop. Universal OSS:
[libpopcnt](https://github.com/kimwalisch/libpopcnt),
[sse-popcount](https://github.com/WojciechMula/sse-popcount),
[SimSIMD](https://github.com/ashvardanian/SimSIMD), FAISS
`IndexBinaryFlat`.
→ **Strong fit at N ≲ 5 000. Required baseline regardless.**

### 1973 — BK-tree (Burkhard & Keller, CACM)

Metric tree on discrete metrics; children grouped by exact distance to
parent; triangle-inequality prune on descent. Sublinear in practice for
small `r`, worst-case linear. SymSpell benchmarks show it ~100× slower
than newer methods for spell-check; comparable story for fixed-length
Hamming. Easy port, no deps. Canonical writeup:
[Damn Cool Algorithms #1 (2007)](http://blog.notdot.net/2007/4/Damn-Cool-Algorithms-Part-1-BK-Trees).
→ **Dominated. Keep as a zero-dep fallback only.**

### 1991/1993 — VP-tree (Uhlmann IPL; [Yianilos SODA](http://algorithmics.lsi.upc.edu/docs/practicas/p311-yianilos.pdf))

Vantage point + median radius partition the space; recurse. Designed
for *expensive* metrics — node prune amortizes distance computation.
Hamming on `u32` is one instruction, so the prune overhead exceeds the
scan it saves. → **Skip.**

### 1992 — Bitap / shift-or / agrep (Baeza-Yates & Gonnet, CACM; Wu & Manber)

Bit-parallel approximate substring search; ancestor of all
"comparison-as-bitops" techniques. Different problem (substring with
errors, not NN over a dictionary), but every fast method since —
universal Levenshtein automata, FAISS's popcount kernels, our own
inner loop — applies the same principle. → **Inspiration, not a fit.**

### 1998 — LSH bit-sampling ([Indyk & Motwani, STOC](https://arxiv.org/abs/cs/9812008))

Hash on `k` random bit positions; `L` such tables; bucket-mates are
candidates. Approximate, sublinear `O(N^ρ)` with `ρ = 1/c`
(O'Donnell-Wu-Zhou 2009 proved optimal). Shines at high `b`; at `b=32`
every other approach dominates. FAISS `IndexLSH`,
[E2LSH](https://www.mit.edu/~andoni/LSH/).
→ **Foundational, not for us.**

### 2002 — Universal Levenshtein automata ([Schulz & Mihov, IJDAR](https://dmice.ohsu.edu/bedricks/courses/cs655/pdf/readings/2002_Schulz.pdf); [2004 follow-up](https://aclanthology.org/J04-4003.pdf))

Build a DFA accepting all strings within Levenshtein-`n` of the query;
intersect on the fly with a dictionary trie. Construction O(|W|).
Powers Lucene/Elasticsearch fuzzy search. Levenshtein ≠ Hamming, so
unless we ever mix sub-grid resolutions (variable-length signatures),
not a fit. → **Hold in reserve.**

### 2002 / 2007 — SimHash (Charikar STOC; [Manku/Jain/Das Sarma WWW](https://research.google.com/pubs/archive/33026.pdf))

Charikar: signed random-hyperplane fingerprint for cosine similarity.
Manku at Google: deployed on 8 × 10⁹ docs with 64-bit fingerprints, and
— five years before Norouzi — invented the **table-permutation lookup**
(several rotated copies → radius-`k` queries decompose into prefix
lookups). The lookup trick is the algorithmic core of what later
became MIH; we want it. SimHash itself doesn't generate our signatures
(rasterizer does).
→ **High relevance for dedup; algorithmically MIH supersedes it.**

### 2008/2009 — Permutation index (Chávez, Figueroa, Navarro, TPAMI)

Each point represented as its permutation of distances to `k`
permutants; ranked by permutation-distance. Same logic as VP-tree:
designed for expensive metrics. → **Skip.**

### 2010/2011 — Product Quantization ([Jégou, Douze, Schmid, TPAMI](https://inria.hal.science/inria-00514462v1/document))

Decompose vector → sub-vectors → quantize each against a learned
codebook → compact code, distance via LUT. Compresses *real-valued*
vectors; ours are already binary at the granularity we want. Binary
PQ exists (Gong et al.) but solves a different problem. Foundation of
all FAISS non-binary indices, ScaNN, Milvus. → **Not applicable
unless we move to continuous sub-pixel features.**

### 2012 — [SymSpell](https://github.com/wolfgarbe/SymSpell) (Wolf Garbe)

Pre-generate every *delete-only* variant ≤ `d` for each dict word →
hash table. Query generates its own delete-variants → lookup. Trick
exploits string *length* asymmetry; analogous trick for fixed-length
Hamming is just **enumerated radius-`r` flips**:

| b=32 | r=1 | r=2 | r=3 | r=4 |
|---|---|---|---|---|
| C(b,≤r) keys/code | 33 | 528 | 5 489 | 41 449 |

At `N=10⁶, r=2`: 5 × 10⁸ keys → ~6 GB (`u32→u32`). At `r=1`: trivial.
**Reported 100× faster than BK-tree on spell-check workloads.**
→ **Mechanism transferable. Pre-compute hash for the radius our noise
budget needs.**

### 2012/2014 — Multi-Index Hashing ([Norouzi/Punjani/Fleet, arXiv:1307.2982](https://arxiv.org/abs/1307.2982); [impl](https://github.com/norouzi/mih))

Split each code into `m` disjoint substrings of `b/m` bits → `m` hash
tables. **Pigeonhole:** any Hamming-`r` neighbor matches the query in
some chunk with radius `⌊r/m⌋`. Probe each chunk's table within that
radius, union, verify on full `b`. **Exact k-NN, sublinear
`O(N^{1-r/(b·m)})` for uniform codes.** For our `b=32`, `m=4` → four
256-bucket tables (cache-resident). Probe cost:

| r | per-chunk radius | buckets probed |
|---|---|---|
| ≤3 | 0 | ≤4 |
| 4–7 | 1 | ≤36 |
| 8–11 | 2 | ≤148 |

Adopted as FAISS `IndexBinaryMultiHash` per the
[FAISS Binary Hashing benchmark](https://github.com/facebookresearch/faiss/wiki/Binary-hashing-index-benchmark).
→ **Primary recommendation for N ≳ 10⁴.**

### 2013 — HmSearch ([Zhang/Qin/Wang/Sun/Lu, SSDBM](https://dl.acm.org/doi/abs/10.1145/2484838.2484842); [impl](https://github.com/commonsmachinery/hmsearch))

Improved enumeration signatures + hierarchical filter/verify; ~2 orders
of magnitude over MIH on the authors' workload, but later EDBT work
(Tang 2015) shows it doesn't scale past ~10M codes (replication blowup).
Gains matter at `b ≥ 64`; at `b=32` MIH's substring tables are already
small.
→ **Marginal over MIH for us.**

### 2016/2018 — HNSW ([Malkov & Yashunin, arXiv:1603.09320](https://arxiv.org/abs/1603.09320))

Multi-layer proximity graph; logarithmic greedy descent. Distance-
metric-agnostic. **Dominant ANN in production today** (Meta, Spotify,
Pinecone, Weaviate, Milvus, Qdrant; consistently top of
[ann-benchmarks](http://ann-benchmarks.com/)). FAISS binary benchmark:
HNSW wins at *large* radii (>30 on 256-bit); MIH wins at small radii —
and ours are small. Implementations:
[hnswlib](https://github.com/nmslib/hnswlib),
[rust-cv/hnsw](https://github.com/rust-cv/hnsw),
FAISS `IndexBinaryHNSW`.
→ **Wrong tool for low `b`, low `r`. Reserve for high-res signatures.**

### 2017+ — FAISS Binary indices (Meta; [library paper 2024](https://arxiv.org/abs/2401.08281))

Library bundling the algorithms above for binary vectors:
`IndexBinaryFlat` (popcount scan), `IndexBinaryIVF` (inverted file),
`IndexBinaryHash` (Manku-style), `IndexBinaryMultiHash` (MIH),
`IndexBinaryHNSW`. Requires `b` divisible by 8 — our 32 fits.
[AWS reports 8× AVX-512 speedup](https://aws.amazon.com/blogs/big-data/save-big-on-opensearch-unleashing-intel-avx-512-for-binary-vector-performance/).
Designed mostly for `b ≥ 64`; heavy C++ dep.
→ **Algorithmic reference, not a dependency.**

### 2019 — DiskANN / Vamana ([Subramanya et al., NeurIPS](https://suhasjs.github.io/files/diskann_neurips19.pdf))

Flatter graph than HNSW; partial index stays on SSD with working set in
RAM. Targets billion-scale on a single node — SQL Server 2025 ships it.
At our N everything fits in RAM trivially.
→ **Solves a problem we don't have.**

### 2020/2022 — Xor / [Binary Fuse filters](https://arxiv.org/abs/2201.01174) (Graf & Lemire)

Probabilistic set membership at ~9 bits/entry (vs Bloom ~13). Useful if
dedup needed to ask "seen this signature?" against a set too big for
RAM — ours never is; `HashSet<u32>` wins.
[xor_singleheader](https://github.com/FastFilter/xor_singleheader),
[xorf crate](https://docs.rs/xorf).
→ **Not for us.**

---

## Recommendation

Fit matrix (★★★ strong, ★ dominated, – wrong tool):

| Method | N≈250 | N≈10⁴ | N≈10⁶ | Exact |
|---|---|---|---|---|
| SIMD popcount scan | ★★★ | ★★ | ★ | yes |
| Enumerated radius-`r` hash | ★★ | ★★★ | ★★★ (r≤2) | yes within r |
| MIH (`m=4`, 8-bit chunks) | ★★ | ★★★ | ★★★ | yes |
| BK-tree | ★ | ★ | ★ | yes |
| LSH / HNSW / PQ / DiskANN | – | – | ★ approx | no |

### Roadmap

1. **Now (N≈250):** AVX2/AVX-512 the existing scan. 5–10× free; no
   algorithmic change.
2. **Post-Unicode-dedup (N≈10⁴–10⁶):** implement MIH with `m=4`.
   ~40 lines of Rust, four 256-bucket tables, cache-resident, exact.
3. **Dedup pass (one-shot, offline):** brute popcount, ~155 k × 250 =
   4 × 10⁷ comparisons → ms on AVX-512.
4. **If signatures ever grow to ≥ 64 bits:** revisit
   `IndexBinaryHNSW` and DiskANN.

### Out of scope and why

- **GPU NN (FAISS-GPU)** — matcher runs on the SSH server's CPU.
- **Approximate NN with c-approx bounds** — glymo's output is already
  lossy by design; approximating Hamming adds loss with no upside.
- **Learned indices (Kraska et al.)** — target B-tree replacement, not
  metric NN.

---

# MIH deep-dive

## What it does

Given a database of `N` binary codes of width `b` and a query `q`, MIH
returns every code within Hamming distance `r` of `q` (and therefore the
exact k-nearest) without scanning the whole database. The trick is
purely combinatorial: a hash table can only find *exact* matches, but
if you split each code into chunks and hash each chunk separately, a
*near*-match in the whole code must contain at least one *closer*-match
in some chunk. You harvest the chunk-level near-matches, deduplicate,
and verify on the full code.

## Why it works (the pigeonhole)

Split every `b`-bit code into `m` disjoint chunks of `b/m` bits. If
`Hamming(q, x) ≤ r`, the `r` differing bits are distributed across the
`m` chunks. By pigeonhole, **at least one chunk holds at most ⌊r/m⌋
differing bits.** So if we probe every chunk's hash table for keys
within radius `⌊r/m⌋` of the query's chunk, `x` is guaranteed to land in
the candidate set of at least one chunk. Verification on the full `b`
bits filters out false positives.

The clever part is that `⌊r/m⌋` is *much* smaller than `r`. At `b=32,
m=4, r=3`: per-chunk radius is 0 — a single bucket lookup. At `r=7`,
per-chunk radius is 1 — nine buckets per chunk.

## Our configuration

`b = 32, m = 4` ⇒ each chunk is 8 bits, each table has 256 buckets:

```text
code  31………24 23………16 15……… 8  7……… 0
chunk    c3      c2      c1      c0       (each c_i is u8)
tables   T3      T2      T1      T0       (T_i[c_i] → list of code IDs)
```

Choice of `m=4` is set by the byte boundary: 8-bit chunks give 256-bucket
arrays addressable with the byte itself — no hashing, just indexing.
Smaller `m` (e.g. 2, with 16-bit chunks) means 65k buckets per table
(sparse for our N) and a coarser pigeonhole (per-chunk radius scales as
`r/2` not `r/4`, so each chunk needs more probes). Larger `m` (e.g. 8,
4-bit chunks) means tighter pigeonhole but more chunks and more
duplication of code IDs across tables. `m=4` is the natural pick.

## Build (offline, one-shot)

```rust
struct Mih32 {
    pool:   Vec<u32>,                // the signatures
    tables: [[Vec<u32>; 256]; 4],    // 4 tables × 256 buckets of code IDs
}

fn build(pool: Vec<u32>) -> Mih32 {
    let mut tables: [[Vec<u32>; 256]; 4] = std::array::from_fn(|_| {
        std::array::from_fn(|_| Vec::new())
    });
    for (id, &code) in pool.iter().enumerate() {
        let id = id as u32;
        for t in 0..4 {
            let chunk = (code >> (8 * t)) as u8 as usize;
            tables[t][chunk].push(id);
        }
    }
    Mih32 { pool, tables }
}
```

Storage:
- 4 × 256 = 1024 bucket headers (24 bytes each on 64-bit) = 24 KB fixed.
- Code IDs in buckets: `4 × N × 4` bytes (every code appears once per
  table). For `N = 10⁴`: 160 KB. For `N = 10⁶`: 16 MB.
- Pool itself: `4N` bytes.

Total for `N = 10⁴`: **~200 KB** — L2-resident on every server CPU.
Total for `N = 10⁶`: **~20 MB** — bigger than L2 but the per-query
working set is much smaller (next section).

## Query (the hot loop)

For 1-NN — what `match_cell` needs — grow `r` until verified:

```rust
fn nearest(&self, q: u32, scratch: &mut Vec<u8>) -> u32 {
    scratch.clear();
    scratch.resize(self.pool.len(), 0);

    let mut best_d  = u32::MAX;
    let mut best_id = 0u32;

    // Radius-0 probe per chunk → 4 buckets total.
    for t in 0..4 {
        let chunk = (q >> (8 * t)) as u8 as usize;
        for &id in &self.tables[t][chunk] {
            if scratch[id as usize] != 0 { continue; }
            scratch[id as usize] = 1;
            let d = (q ^ self.pool[id as usize]).count_ones();
            if d < best_d { best_d = d; best_id = id; }
        }
    }
    // Pigeonhole guarantee: any code with Hamming ≤ 3 was in the
    // radius-0 candidate set. If best_d ≤ 3, we're exact.
    if best_d <= 3 { return self.pool[best_id as usize]; }

    // Otherwise expand per-chunk radius to 1 (covers r ≤ 7), etc.
    // ...
}
```

Step-by-step per query at `r ≤ 3`:

1. **4 chunk extractions** — shift + mask, 4 cycles.
2. **4 bucket reads** — 4 pointer chases into `Vec<u32>`. Bucket
   pointers are in 4 KB, candidate IDs in tens to hundreds of bytes —
   one cache miss at most.
3. **Dedup** via a `seen` byte vector. Allocate once, reuse across
   queries; reset by `clear()`+`resize()` or a per-query generation
   counter to skip the zero-fill.
4. **Verify** each unique candidate: `pool[id] ^ q`, `count_ones()`,
   track min. This is the inner loop that SIMD-vectorises 8-wide on
   AVX2 / 16-wide on AVX-512.

Number of verifies per query, assuming uniform 8-bit chunk distribution
(true post-dedup; the chunks are effectively hashes of arbitrary glyph
bitmaps):

| N | avg candidates per bucket | r=0–3 (4 probes) | r=4–7 (36 probes) |
|---|---|---|---|
| 250 | 1 | ~4 | ~36 |
| 10⁴ | 39 | ~160 | ~1 400 |
| 10⁵ | 390 | ~1 600 | ~14 000 |
| 10⁶ | 3 900 | ~16 000 | ~140 000 |

Compare to brute force, which always verifies `N`. Speedup at the
typical r=0–3 regime: **~60× at N=10⁴, ~60× at N=10⁶** (the ratio is
constant because both numerator and denominator scale with N).

## Why r is typically small

The matcher always wants 1-NN. The radius it actually has to search
isn't the *noise budget* — it's the *true NN distance* in the pool. For
a dense post-Unicode pool of ~10⁴–10⁵ unique 32-bit signatures, code
density is high enough that 1-NN almost always sits within Hamming
distance 2–3 of any query (the `2³² / N` bucket density argument: at
`N = 10⁵`, the average code has ~10⁵ neighbors within Hamming-8; far
more than enough for several within Hamming-3). So the r=0–3 path
(4 bucket probes per query) is the common case; r ≥ 4 is rare.

If the query lands in a pool void (rare; happens on degenerate patches),
incremental expansion costs more but still beats brute by a wide margin
through r ≈ 7.

## Expected performance vs the current matcher

Per-query, no SIMD:

| N | brute (`matcher.rs` today) | MIH r=0–3 | speedup |
|---|---|---|---|
| 250 | ~250 popcount-XOR ≈ 500 cyc | ~4 verifies + 50 cyc overhead ≈ 60 cyc | ~8× |
| 10⁴ | ~10⁴ ≈ 20 000 cyc | ~160 verifies + overhead ≈ 400 cyc | **~50×** |
| 10⁶ | ~10⁶ ≈ 2 000 000 cyc | ~16 000 verifies + overhead ≈ 35 000 cyc | **~55×** |

With AVX-512 popcount on both sides (`VPOPCNTQ` over packed u32s), the
brute baseline drops ~8× but so does MIH's verify step, and the
speedup ratio stays roughly the same — the structural win is the
candidate-set reduction, not the inner-loop op.

At the full-broadcast load (~10⁸ matcher calls/sec aggregate across
sessions), 35 µs/call at N=10⁶ × 10⁸ calls = 3.5 × 10³ core-seconds/sec
= 3500 cores. With MIH that drops to ~60 cores. **MIH makes the
post-Unicode pool size viable; brute force does not.**

## What MIH gives up

- **Build cost.** O(N) to populate the tables. One-time at process
  start (or on pool change). Trivial.
- **Memory.** ~5× the pool itself. At N=10⁶ that's 20 MB total — fine.
- **No knob for approximate-NN.** MIH is always exact within its
  radius. If a query needs a wider radius and we're CPU-constrained,
  we can cap the per-chunk radius (`floor(r/m)`) and accept missing the
  rare codes that only land at, say, r ≥ 8 — but the noise model says
  we won't need that.
- **Build is per-pool.** If the pool ever changes at runtime (it
  shouldn't), rebuild is needed; incremental insert/delete is
  straightforward (just push/remove from the 4 buckets) but unused for
  us.

## Sketch of the dedup pass

For the offline "render all of Unicode and collapse duplicates" task,
MIH isn't even necessary — `HashMap<u32, char>` is faster (single
hash, no chunk dance), and the input set is ~10⁵ codepoints. The
build of MIH itself essentially *is* the dedup, since we'd discard
duplicate signatures while inserting. Use MIH only for the runtime
matcher; dedup is a one-shot `HashMap` insert.

---

*Font-survey content (which font to rasterize from, why DejaVu Sans Mono) was originally part of this document and has been split out to [`font-survey.md`](font-survey.md).*
