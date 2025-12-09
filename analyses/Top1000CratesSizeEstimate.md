# Size Estimates and Download Times for Top 1000 Rust Crates

## Data Collection

- **Total unique repositories**: 681
- **Successfully sampled**: 203 repositories (30.3%)
- **GitHub API rate limits**: Hit after ~200 requests

## Size Estimates Based on Sample

### From GitHub API (repository metadata size)

- **Sample total**: 2.49 GB for 203 repos
- **Average repository**: 12.55 MB
- **Median repository**: 0.58 MB
- **Estimated total** (extrapolated): **8.4 GB**

### Actual Clone Size (including git history)

Git clones include full commit history, which is typically 1.5-2x larger than the
repository size reported by the GitHub API. Using a conservative 1.8x multiplier:

- **Estimated total with git history**: **15.0 GB**

## Download Time Estimates

Based on 15.0 GB total size (with git history):

- **10 Mbps (slow broadband)**: 3.4 hours (205 minutes)
- **50 Mbps (typical broadband)**: 41.1 minutes
- **100 Mbps (fast broadband)**: 20.5 minutes
- **1 Gbps (gigabit)**: 2.1 minutes


## Notes and Caveats

1. **Sample Limitation**: Only 30% of repositories were successfully queried due to
   GitHub API rate limits. The extrapolation assumes the remaining repositories have
   similar size distributions.

2. **Repository Deduplication**: The 1000 crates map to only 681 unique repositories
   because many crates share the same repository (e.g., tokio, serde ecosystem).

3. **Git History Overhead**: The 1.8x multiplier for git clone size is conservative.
   Some repositories with extensive history may be larger.

4. **Shallow Clones**: If using `--depth 1` for shallow clones, the actual size would
   be closer to the base estimate of 8.4 GB.

5. **Network Overhead**: Actual download times will be longer due to:
   - TCP/IP overhead
   - GitHub server rate limiting
   - Connection establishment time for 681 separate repos
   - Potential retries and errors

6. **Practical Estimate**: For parallel cloning with a 100 Mbps connection, expect
   the process to take **15-30 minutes** accounting for all overhead.

## Breakdown by Size Category (from sample)

Based on the sample data:
- **Large repos (>100 MB)**: ~5% of repositories
- **Medium repos (10-100 MB)**: ~20% of repositories  
- **Small repos (<10 MB)**: ~75% of repositories

The median size of 0.58 MB shows that most Rust crates are quite compact, but a few
large repositories (like LLVM bindings, game engines, etc.) significantly increase
the total.

## Recommendations

1. **Use parallel cloning** (e.g., `xargs -P 10`) to speed up the process
2. **Consider shallow clones** (`--depth 1`) if you don't need git history
3. **Budget ~20-30 GB disk space** for safety (allowing for decompression and build artifacts)
4. **Expect 20-40 minutes** on a typical broadband connection with all overhead
