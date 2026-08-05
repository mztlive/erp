#!/usr/bin/env python3
"""P0-6.3: 校验 docs/dev-plan/_meta.json 的机器可读约束。

校验项（P0-foundation.md §6.3，分支名与 owns 前缀无重叠）：
  1. stages 的 id 与 branch 全局唯一；
  2. layers 的 owns_pattern 展开后（{module} 代入 domains 全部 34 域）：
     - 跨批次/跨层不存在重叠文件前缀（同层同批次内允许）；
     - P0 冻结文件清单（frozen_files，与 conventions.md 第 2 节一致）
       与所有 owns_pattern 展开结果不得重叠（任何 owns 不得指向冻结文件）；
  3. stages.depends_on 引用的 id 都存在且无环（DFS）；
  4. domains 的 deps 引用都存在；
  5. batches 引用的 domains 都存在。

纯标准库（json/sys），无第三方依赖：
    python3 docs/dev-plan/check-meta.py

全部通过退出 0，任一失败退出 1。
"""

import json
import os
import sys

META_PATH = os.path.join(os.path.dirname(os.path.abspath(__file__)), "_meta.json")

# 每个 owns_pattern 展开为若干"路径模式"。路径模式按 / 切分为元组，
# 其中 {module} 已被具体模块名替换，叶子项为文件名（含后缀）或目录名。
# 目录前缀用 ("backend", "entities", "src", "common") 这类 open 元组表示：
# 它匹配以该元组为前缀的任意路径。


def split_pattern(path: str):
    """按 / 切分路径为组件元组；空组件（如尾斜杠）被剔除。"""
    return tuple(c for c in path.split("/") if c)


def load_meta():
    with open(META_PATH, encoding="utf-8") as f:
        return json.load(f)


def expand_owns(layers, domains):
    """把 layers.owns_pattern 展开为 (layer, batch, domain, path_tuple) 列表。

    同一 (layer, domain) 的多个 pattern 各占一行；batch 取 domains[batch]。
    层 P0/P4/P5 无 owns_pattern，跳过。
    """
    expanded = []
    for layer_name, layer in layers.items():
        patterns = layer.get("owns_pattern")
        if not patterns:
            continue
        for domain_id, domain in domains.items():
            module = domain["module"]
            batch = domain["batch"]
            for pattern in patterns:
                replaced = pattern.replace("{module}", module)
                expanded.append((layer_name, batch, domain_id, split_pattern(replaced)))
    return expanded


def is_prefix(short, long):
    """short 是否组件级前缀（含相等）于 long。"""
    if len(short) > len(long):
        return False
    return long[: len(short)] == short


def check_uniqueness(stages):
    """校验项 1：stages.id 与 branch 全局唯一。"""
    problems = []
    ids = {}
    branches = {}
    for stage in stages:
        sid = stage["id"]
        branch = stage.get("branch")
        if sid in ids:
            problems.append(f"  [FAIL] stages.id 重复: {sid}（{ids[sid]} 与当前条目）")
        else:
            ids[sid] = branch
        if branch:
            if branch in branches:
                problems.append(f"  [FAIL] stages.branch 重复: {branch}（{branches[branch]} 与 {sid}）")
            else:
                branches[branch] = sid
    if not problems:
        print(f"  [OK] stages.id 唯一（{len(ids)} 个）；branch 唯一（{len(branches)} 个）")
    else:
        print("  [FAIL] id/branch 唯一性：")
        for p in problems:
            print(p)
    return not problems


def check_owns_overlap(expanded):
    """校验项 2a：跨批次/跨层 owns 前缀不得重叠（同层同批次允许）。"""
    problems = []
    n = len(expanded)
    checked = 0
    for i in range(n):
        li, bi, di, pi = expanded[i]
        for j in range(i + 1, n):
            lj, bj, dj, pj = expanded[j]
            same_group = li == lj and bi == bj
            if same_group:
                continue
            if is_prefix(pi, pj) or is_prefix(pj, pi):
                problems.append(
                    f"  [FAIL] owns 前缀重叠: {li}/{bi}/{di} {'.'.join(pi)} 与 "
                    f"{lj}/{bj}/{dj} {'.'.join(pj)}"
                )
            checked += 1
    if not problems:
        print(f"  [OK] owns_pattern 展开后无跨批次/跨层前缀重叠（共比对 {checked} 对）")
    else:
        print("  [FAIL] owns_pattern 展开后存在跨批次/跨层重叠：")
        for p in problems:
            print(p)
    return not problems


def check_frozen_disjoint(expanded, frozen_files):
    """校验项 2b：冻结文件清单与所有 owns_pattern 展开结果不得重叠。"""
    frozen = [split_pattern(f) for f in frozen_files]
    problems = []
    for layer, batch, domain_id, path in expanded:
        for fz in frozen:
            if is_prefix(fz, path) or is_prefix(path, fz):
                problems.append(
                    f"  [FAIL] owns 与冻结文件重叠: {layer}/{batch}/{domain_id} "
                    f"{'.'.join(path)} ↔ 冻结 {'.'.join(fz)}"
                )
    if not problems:
        print(f"  [OK] 冻结文件清单（{len(frozen)} 条）与所有 owns_pattern 展开无重叠")
    else:
        print("  [FAIL] owns_pattern 指向冻结文件：")
        for p in problems:
            print(p)
    return not problems


def check_depends_acyclic(stages):
    """校验项 3：depends_on 引用存在且无环（DFS）。"""
    ids = {s["id"] for s in stages}
    by_id = {s["id"]: s.get("depends_on") or [] for s in stages}
    missing = []
    for sid, deps in by_id.items():
        for dep in deps:
            if dep not in ids:
                missing.append(f"  [FAIL] stage {sid} depends_on 引用不存在的 id: {dep}")
    if missing:
        print("  [FAIL] depends_on 引用：")
        for m in missing:
            print(m)
        return False

    state = {}  # 0=未访问 1=访问中 2=完成
    cycle = []

    def dfs(sid, stack):
        state[sid] = 1
        stack.append(sid)
        for dep in by_id[sid]:
            if state.get(dep) == 1:
                idx = stack.index(dep)
                cycle.append(stack[idx:] + [dep])
                return
            if state.get(dep) != 2:
                dfs(dep, stack)
        stack.pop()
        state[sid] = 2

    for sid in by_id:
        if state.get(sid) != 2:
            dfs(sid, [])

    if not cycle:
        print(f"  [OK] depends_on 全部存在且无环（{len(ids)} 个 stage）")
    else:
        print("  [FAIL] depends_on 存在环：")
        for c in cycle:
            print("  环: " + " -> ".join(c))
    return not cycle


def check_domain_deps(domains):
    """校验项 4：domains.deps 引用都存在。"""
    ids = set(domains)
    problems = []
    for did, info in domains.items():
        for dep in info.get("deps") or []:
            if dep not in ids:
                problems.append(f"  [FAIL] domain {did} deps 引用不存在的 domain: {dep}")
    if not problems:
        print(f"  [OK] domains.deps 全部引用存在（{len(ids)} 个 domain）")
    else:
        print("  [FAIL] domains.deps 引用：")
        for p in problems:
            print(p)
    return not problems


def check_batches(domains, batches):
    """校验项 5：batches 引用的 domains 都存在，且每个 domain 的 batch 存在。"""
    problems = []
    for bid, info in batches.items():
        for dep in info.get("domains") or []:
            if dep not in domains:
                problems.append(f"  [FAIL] batch {bid} 引用不存在的 domain: {dep}")
    for did, info in domains.items():
        if info.get("batch") not in batches:
            problems.append(f"  [FAIL] domain {did} 引用不存在的 batch: {info.get('batch')}")
    if not problems:
        print(f"  [OK] batches 引用的 domains 全部存在（{len(batches)} 个 batch，{len(domains)} 个 domain）")
    else:
        print("  [FAIL] batches/domains 引用：")
        for p in problems:
            print(p)
    return not problems


def main():
    meta = load_meta()
    print(f"校验 {META_PATH}")
    print(f"  - version: {meta.get('version')}  scope: {meta.get('scope')}")
    print(f"  - stages: {len(meta['stages'])}   domains: {len(meta['domains'])}   batches: {len(meta['batches'])}")

    results = []
    print("\n[1/5] stages.id / branch 全局唯一")
    results.append(check_uniqueness(meta["stages"]))

    expanded = expand_owns(meta["layers"], meta["domains"])
    print(f"\n[2/5] owns_pattern 重叠与冻结文件检查（展开 {len(expanded)} 个路径模式）")
    results.append(check_owns_overlap(expanded))
    results.append(check_frozen_disjoint(expanded, meta["frozen_files"]))

    print("\n[3/5] stages.depends_on 存在性与无环")
    results.append(check_depends_acyclic(meta["stages"]))

    print("\n[4/5] domains.deps 引用存在性")
    results.append(check_domain_deps(meta["domains"]))

    print("\n[5/5] batches 引用 domains 存在性")
    results.append(check_batches(meta["domains"], meta["batches"]))

    ok = all(results)
    print(f"\n结论: {'全部通过' if ok else '存在失败项'}")
    sys.exit(0 if ok else 1)


if __name__ == "__main__":
    main()
