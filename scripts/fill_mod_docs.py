#!/usr/bin/env python3
# Insert a module-level //! doc (based on top-level pub items) into .rs files
# that lack one. Reports changed files at the end.
import os

ROOT = 'src'
ORDER = ['trait', 'struct', 'enum', 'fn', 'mod', 'type', 'const', 'static', 'use']


def has_mod_doc(txt):
    for line in txt.splitlines():
        s = line.lstrip()
        if not s:
            continue
        if s.startswith('//!') or s.startswith('/*!'):
            return True
        if not s.startswith('//') and not s.startswith('#'):
            return False
    return False


def top_pubs(txt):
    kinds = {k: [] for k in ORDER}
    for line in txt.splitlines():
        s = line.lstrip()
        if not s.startswith('pub '):
            continue
        body = s[4:].lstrip()
        while body.startswith('unsafe ') or body.startswith('async '):
            body = body.split(' ', 1)[1].lstrip()
        kw = body.split(' ', 1)[0] if body else ''
        if kw not in kinds:
            continue
        rest = body[len(kw):].lstrip()
        name = rest.split('(')[0].split('<')[0].split('{')[0].split(';')[0].split('=')[0].split(':')[0].strip()
        if name:
            kinds[kw].append(name)
    return kinds


def dedupe(names):
    seen = []
    for n in names:
        if n not in seen:
            seen.append(n)
    return seen


changed = []
for dp, _, fns in os.walk(ROOT):
    for fn in sorted(fns):
        if not fn.endswith('.rs'):
            continue
        fp = os.path.join(dp, fn)
        with open(fp, encoding='utf-8') as fh:
            txt = fh.read()
        if has_mod_doc(txt):
            continue
        rel = os.path.relpath(fp, ROOT)
        modpath = os.path.splitext(rel)[0].replace(os.sep, '::')
        pubs = top_pubs(txt)
        parts = []
        for k in ORDER:
            names = dedupe(pubs[k])
            if names:
                parts.append(k + ' ' + ', '.join(names[:8]))
        if parts:
            desc = '公开接口：' + '；'.join(parts)
        else:
            desc = '内部实现模块（无公开顶层项）'
        doc = '//! `' + modpath + '` 模块。' + desc + '。' + chr(10) + chr(10)
        with open(fp, 'w', encoding='utf-8') as fh:
            fh.write(doc + txt)
        changed.append((fp, desc))

print('changed=%d' % len(changed))
for fp, desc in changed:
    print('-', fp, '::', desc[:90])
