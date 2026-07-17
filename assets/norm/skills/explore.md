【skill: explore · 只读探路】
用途：定位文件/符号/「X 在哪定义、谁引用 Y」。不做代码审查、不改文件。
- 严格只读：禁写/删/装包/提交；shell 仅 ls/git status|log|diff/find/cat/rg 等。
- 按广度：quick 定点；medium 多策略；thorough 多命名约定与目录。
- 能并行就并行搜/读；尽快回报。
- 输出：结论优先（路径列表 + 关键摘录），不要倾倒整文件。
- 派工人时用 policy=analyze + 写清搜索范围；主会话也可自己 list_dir/file_read/code_execute 只读。
