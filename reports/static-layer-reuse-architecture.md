# PERF-003：静态子树复用的跨帧像素缓存架构说明

状态：仅架构说明与判定契约，未接线到帧循环；默认像素路径零漂移。

基准：Flutter stable 3.50.0，framework `ab2912035e160d9cad6306df307576da6128ebdb`，
engine `371f77202a73e8dff57a8b3bff75bca55c090aee`。Skia 子模块本机未拉取
（pinned `0e5d8d1be81b22f5529a41cf4810dd89b63527ba`，`DEPS:18,505`），因此这里只
平移 framework 的 retained-layer 判定机制，不声称 Skia 像素实现已经对齐。

## 1. Flutter 权威依据

- `packages/flutter/lib/src/rendering/layer.dart:351-372,374-389`：`_needsAddToScene`
  初始为 true；`markNeedsAddToScene` 在已脏时短路。
- `layer.dart:409`：`alwaysNeedsAddToScene` 层每次都重合成，不能缓存。
- `layer.dart:444-478`：只有可保留 engine layer 的父层会因子层 engine layer 变化变脏。
- `layer.dart:1158-1168`：`updateSubtreeNeedsAddToScene` 对子树脏标记取或。
- `layer.dart:860-912`：raster cache hint 允许组件禁止缓存；本说明以“有特效/动画即禁止”保守等价。
- `packages/flutter/lib/src/rendering/object.dart:1263-1326,3344-3368`：重绘边界收集
  和绘制脏标记止于边界，划定可复用区域的上界。

Opurom 不引入 Layer/RenderObject 运行时对象树；对应机制已经落在纯函数
`src/layer.rs::LayerDirtyState` 与 `StaticLayerIdentity::can_reuse`。

## 2. 建议的缓存模型（尚未实现）

1. 缓存对象不是 engine layer，而是完成上一帧后保留的一份 RGBA 帧缓冲；
   复用动作是 `copy_rect(previous, current, node_box)` 的确定像素拷贝。
2. 复用条件：`StaticLayerIdentity::can_reuse(current, previous, dirty_state)`
   为 true，即当前/上一帧都不含动画、subtree_hash 相同、层干净且 cacheable。
3. 候选节点必须整体落在已计算出的脏区域内，且节点自身及其子树没有 local
   effect、粒子、文字次像素 AA 依赖或半透明跨节点叠加；这些一律回退重绘。
4. 拷贝顺序按当前 display list 的绘制顺序。任何会与缓存区域相交的后续命令
   都必须重绘，避免覆盖顺序错误。
5. 帧结束时更新 retained identity 与 dirty state：本帧真正重绘的节点标脏，
   其余节点继承 clean；只有 `finish_add_to_scene` 之后才允许下一帧复用。

## 3. 不接线条件

- 未证明复用路径与全量光栅逐字节一致前，开关默认关闭。
- 脏区域内的静态兄弟节点若与半透明或 effect 命令相交，禁止复用。
- 按按钮/涟漪（PERF-004）导致的上一个 press disc 区域未清理前，禁止复用。
- 缓存只保存已完成帧，不保存中间合成状态；resize、主题/字体变化使所有
  identity 失效，退回全量光栅。

## 4. 验证门槛（未来接线前必须满足）

- 以同一 IR 跑全量与 opt-in 复用，`render_to_pixels` 逐字节相同。
- golden：accessibility、render_quality、incremental_pipeline 以及 press 专项全部零漂移。
- 新增差分测试覆盖：hash 改变、动画节点、脏状态未清理、effect 相交、resize。
- CI：`cargo fmt --check`、`cargo test --offline`、金标与 GPU 回放一致。

## 5. 当前交付边界

本轮仅提供架构约束、审计状态更新和既有纯函数判定契约的文档化。没有向
`Renderer::render_incremental_at` 添加分支、没有新 IR 字段、没有改变任何像素
或 display-list 输出；因此金标保持零漂移。
