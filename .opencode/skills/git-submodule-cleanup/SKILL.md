# Git 子目录清理

清理 Git 子目录（gitlink/子模块）脏状态。用于主仓库出现 `m <子目录>`、子目录有大量本地改动或未跟踪文件、需要恢复干净工作区时。支持安全检查、重置、清理、验证与回滚提示。

按下面顺序执行，目标是把子目录恢复到主仓库记录的提交状态。

## 1) 识别问题

在主仓库运行：

```powershell
git status --short
```

如果看到类似：

- `m tauri-plugin-mcp`

表示子目录内部有本地改动或未跟踪文件。

## 2) 确认它是 gitlink

```powershell
git ls-files -s tauri-plugin-mcp
```

如果模式是 `160000`，说明它是 gitlink。

## 3) 查看子目录内部改动

```powershell
git -C tauri-plugin-mcp status --short
```

## 4) 一键清理子目录

```powershell
git -C tauri-plugin-mcp reset --hard HEAD
git -C tauri-plugin-mcp clean -fd
```

如果只想预览将删除哪些未跟踪文件，先跑：

```powershell
git -C tauri-plugin-mcp clean -fdn
```

## 5) 验证主仓库是否干净

```powershell
git status --short
```

理想结果：无输出。

## 6) 常见异常处理

### A. 子目录不存在 `.git`

说明它可能不是独立仓库或结构异常，先停下，不要强删。优先检查：

```powershell
git submodule status
```

### B. 需要回到主仓库记录的确切提交

```powershell
git submodule update --init --recursive
```

### C. 子目录未来还会频繁变脏

优先做法：

1. 把调试产物写进子目录自己的 `.gitignore`
2. 不在子目录里直接做临时改动
3. 需要改子目录代码时，单独在子目录仓库里提交后，再在主仓库更新 gitlink

## 7) 安全边界

- `reset --hard` 会丢弃子目录已跟踪文件的未提交改动
- `clean -fd` 会删除子目录未跟踪文件
- 执行前如果有要保留的内容，先备份

## 8) 推荐执行模板

```powershell
# 1) 主仓库先确认
git status --short

# 2) 子目录详情
git -C tauri-plugin-mcp status --short

# 3) 清理
git -C tauri-plugin-mcp reset --hard HEAD
git -C tauri-plugin-mcp clean -fd

# 4) 收尾验证
git status --short
```
