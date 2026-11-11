---
name: zeabur-overview-cost
description: Measure what one project overview read costs in bytes, allocations and time, before and after a change to how Zeabur lists or decodes cluster objects. Use when a change claims to save work in zeabur-kube or the overview, or when the overview feels slow or heavy.
---

# Measure the project overview read

The test is `crates/zeabur-kube/tests/overview_cost.rs`. It is `#[ignore]`d, so no pull request runs
it, and it asserts no number.

## What it does

- A fake kube-apiserver on a loopback port answers each LIST with a prebuilt body, so the numbers
  are Zeabur's side of the read.
- One read is `namespace_objects` (12 LISTs) plus the namespace's Events. The file header lists them.
- Two layouts. The baseline is 240 objects of the kinds a shop namespace has, each about the size
  `kubectl get -o json` prints. The 10x layout multiplies every count by ten (2,400 objects).
- A counting allocator (`zeabur_test_support::allocations::measure`, the `allocations` feature)
  counts what the calling thread allocates during the read. The fake server runs on other threads
  and is not counted.

## Run it

```sh
nix develop -c cargo test -p zeabur-kube --release --test overview_cost -- --ignored --nocapture | grep '^|'
```

It prints a markdown table for each layout. Run it on `main` and on your branch, at least twice
each, and put both tables in the pull request.

## Read it

- Bytes allocated is the total over the read. Fewer means less copying and decoding.
- Allocations is how many times memory was requested. It is the steadiest number.
- Peak bytes is the most alive at one moment. The typed result the page keeps dominates it, so it
  often does not move.
- Time is the median of nine reads. On a laptop it is a direction only, since other work on the
  machine moves it by tens of percent. Trust bytes and allocations first.
- The test also reports the cluster calls and HTTP requests. A change that saves work should leave
  them the same, or lower them on purpose.

To change the layout, edit `BASELINE` in the test. Keep the objects realistic, with `managedFields`,
or the savings will look different from a real cluster.
