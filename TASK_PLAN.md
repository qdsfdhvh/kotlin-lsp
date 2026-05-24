# kotlin-lsp 开发任务计划 v0.18

## 🔴 P0 — 测试与稳定性

- [ ] 1. **新功能测试覆盖** _Medium_  
  `parser_tests.rs`: deprecated 检测（`@Deprecated`、`@kotlin.Deprecated`、Java `@Deprecated`）  
  `resolver/tests.rs`: `label_details_from_detail`（params、return type、property、empty）  
  `backend/`: codeAction（add import、suppress warning、generate overrides）

- [ ] 2. **CLI 集成测试** _Medium_  
  `check`: 有效文件 OK / 语法错误 exit 1 / JSON 输出  
  `organize-imports`: 排序、去重、删除未使用  
  `context`: definition + signature 输出  
  `call-hierarchy`: callers 输出  
  `type-hierarchy`: subtypes 输出

- [ ] 3. **`parking_lot::Mutex` 替换** _Low_  
  替换 `std::sync::Mutex` → `parking_lot::Mutex`（无中毒、更快）  
  文件：`src/cli/run.rs`、`src/indexer/apply.rs`、`src/indexer/scan.rs`

## 🟡 P1 — 性能与补全

- [ ] 4. **增量解析** _High_  
  tree-sitter 支持增量 parse（`parser.parse(content, Some(&old_tree))`）  
  大文件编辑时只重新解析变更部分，显著降低 CPU

- [ ] 5. **Java 补全增强** _Medium_  
  `this.` / `super.` 补全  
  继承链方法补全  
  import 自动补全

- [ ] 6. **`textDocument/typeDefinition`** _Low_  
  跳转到变量/属性的类型定义，复用现有 `infer_variable_type`

## 🟢 P2 — 代码质量

- [ ] 7. **减少不必要的 `.clone()`** _Low_  
  `Arc::clone` 大部分合理，检查纯数据 clone  
  用引用替代，或延迟 clone

- [ ] 8. **`Cow<str>` 优化字符串** _Low_  
  函数返回签名时用 `Cow<str>` 避免不必要分配  
  目标：`extract_detail`、`hover` 路径

- [ ] 9. **新增 `CONTRIBUTING.md`** _Trivial_  
  构建、测试、PR 流程、代码规范索引

---

> 进度: **0 / 9** · 创建: 2026-05-24
