import os, re, collections

ROOT = 'src'
OUT = 'docs/src-module-inventory.md'

def file_doc(text):
    m = re.search(r'//!\s*(.+)', text)
    return m.group(1).strip() if m else None

def pub_items(text):
    pat = re.compile(r'\bpub\s+(?:unsafe\s+)?(?:async\s+)?(?:fn|struct|enum|trait|type|const|static|mod)\s+([A-Za-z_][A-Za-z0-9_]*)')
    seen = []
    for m in pat.finditer(text):
        n = m.group(1)
        if n not in seen:
            seen.append(n)
    m2 = re.search(r'\bpub\s+use\s+([A-Za-z_][A-Za-z0-9_]*)', text)
    if m2 and m2.group(1) not in seen:
        seen.append(m2.group(1))
    return seen

groups = collections.defaultdict(list)
total = 0
for root, dirs, files in os.walk(ROOT):
    if 'target' in root:
        continue
    for f in sorted(files):
        if f.endswith('.rs'):
            p = os.path.join(root, f)
            total += 1
            groups[root].append(p)

rows = []
detail_rows = []
for mod in sorted(groups):
    files = sorted(groups[mod])
    mod_items = []
    mod_docs = []
    for p in files:
        text = open(p, encoding='utf-8', errors='replace').read()
        items = pub_items(text)
        for i in items:
            if i not in mod_items:
                mod_items.append(i)
        d = file_doc(text)
        if d:
            mod_docs.append(d)
        detail_rows.append((p, ', '.join(items[:12]) + (' …' if len(items) > 12 else ''), d or '（无 doc 注释）'))
    entry = ', '.join(mod_items[:12]) + (' …共%d项' % len(mod_items) if len(mod_items) > 12 else '')
    duty = ('；'.join(mod_docs[:3])[:200]) if mod_docs else '（无 doc 注释）'
    rows.append((mod, len(files), entry, duty))

lines = []
lines.append('# src/ 模块职责盘点')
lines.append('')
lines.append('自动生成：顶层概览 + 逐文件明细。无 doc 注释的模块如实标注，不编造职责。')
lines.append('')
lines.append('- 总 .rs 文件数：%d' % total)
lines.append('- 模块分组数：%d' % len(rows))
lines.append('- 无 doc 注释的模块数：%d' % sum(1 for r in rows if r[3] == '（无 doc 注释）'))
lines.append('')
lines.append('## 模块概览')
lines.append('| 模块 | 文件数 | 主要入口 | 职责 |')
lines.append('|---|---|---|---|')
for r in rows:
    lines.append('| %s | %d | %s | %s |' % (r[0], r[1], r[2], r[3]))
lines.append('')
lines.append('## 逐文件明细')
lines.append('| 文件 | 主要入口 | doc/职责 |')
lines.append('|---|---|---|')
for p, ent, d in detail_rows:
    lines.append('| %s | %s | %s |' % (p, ent, d))
lines.append('')

open(OUT, 'w', encoding='utf-8').write('\n'.join(lines))
print('WROTE', OUT)
print('FILES', total)
print('MODULES', len(rows))
print('NO_DOC_MODULES', sum(1 for r in rows if r[3] == '（无 doc 注释）'))
print('LINES', len(lines))
print('DETAIL_ROWS', len(detail_rows))
