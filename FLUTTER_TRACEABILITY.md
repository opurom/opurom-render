# Flutter 渲染内核源码溯源台账（render-dev）

基准版本（本机固定只读 checkout，所有行号以此为准）：

- Flutter stable **3.50.0**，framework `ab2912035e160d9cad6306df307576da6128ebdb`，
  engine `371f77202a73e8dff57a8b3bff75bca55c090aee`，Dart 3.14.0。
- checkout：`../flutter-golden/`；Skia 由 `DEPS:18` 固定到
  `0e5d8d1be81b22f5529a41cf4810dd89b63527ba`（`third_party/skia` 子模块本机未拉取，
  涉及 Skia 文件的行号引用保留既有移植注释，待 checkout 后逐行复核）。

## 已落地项

### Material elevation：六级 → Flutter 全十级（2026-11-18）

- 权威源码：`packages/flutter/lib/src/material/shadows.dart:40-221`，
  `kElevationToShadow` 定义 0/1/2/3/4/6/8/9/12/16/24 共 11 个键；
  `:43-45` 定义 umbra `#33000000`、penumbra `#24000000`、ambient `#1F000000`。
- 落地位置：
  - `opurom-render/src/effects.rs`：`ELEVATION_TABLE`（ambient, penumbra, umbra 顺序，
    绘制远→近），全部 10 个非零级别的 offset/blur/spread/alpha 逐项对应上述源码行段；
    `draw_elevation` 的 sigma 换算对应 `engine/src/flutter/lib/ui/painting.dart:8720-8721`
    （`Shadow.convertRadiusToSigma`），Impeller 的 3-sigma 截断对应
    `engine/src/flutter/impeller/geometry/sigma.h:11-22`（`kKernelRadiusPerSigma`）。
  - `opurom-render/src/ir.rs`：`elevation` 属性白名单与上述 11 个键一致。
  - `opurom-render/src/display.rs`、`opurom-gpu/src/display.rs`：`EffectElevation`
    命令白名单与共享表一致；GPU 回放按同一三层参数展开。
  - `opurom-ui/theme/material-tokens.json` 与 `opurom-render/tests/fixtures/material-tokens.json`：
    新增 md-elev-8/9/12/16/24，source 字段标注 shadows.dart 行段。
- 行为约束：状态增量投影 `M(x)`（`src/effects.rs::project_state_level`）保持原六级
  语义不变，避免改变既有交互黄金帧；仅作者显式书写的 elevation 可使用全部十级。
- 验证：`opurom-render/tests/elevation.rs`（源码存在性、token 对齐、十级白名单、
  新增级别像素与命令回归）、`opurom-gpu/tests/elevation.rs`（十级
  6-pass 展开与非法级别拒绝）。2026-11-18：render 666 项、GPU 112 项全过，
  `cargo fmt --check` 通过。

## 待办与已知差距（按模块优先级）

1. AAA：Skia `SkScan_AAAPath.cpp`、`SkAnalyticEdge.{h,cpp}` 子模块未拉取；
   现有移植注释仅有文件名/行号，需在源码可用后补版本号与逐行复核。裁剪边缘、
   填充/描边/阴影共用覆盖仍待统一。
2. 布局：`constraints.dart`/`object.dart` 的 RelayoutBoundary 与
   `RepaintBoundary` 尚未做逐行对照；当前增量管线是自有实现。
3. 合成：`Layer`/`ContainerLayer`/`OpacityLayer` 的源码级对照和静态图层缓存
   尚未完成；预乘 Alpha 已做基础验证（见 `FLUTTER_DETAIL_CHECKLIST.md`）。
4. 文字：`ParagraphStyle`（`engine/src/flutter/lib/ui/text.dart:2031-2140`）、
   `TextHeightBehavior`（`:1487-1580`）、`TextStyle.height`（`:1766-1851`）已定位，
   Opurom 目前仅有 lineHeight 倍率与固定 leadingDistribution，待对齐
   applyHeightToFirstAscent/applyHeightToLastDescent 与 proportional/even 分发。
5. 阴影：10 级表格已对齐；box-blur 尾迹与 Skia Gaussian 仍有已知差异
   （见 `FLUTTER_DETAIL_CHECKLIST.md` 的 2026-11-17 审计记录）。
6. 原语与渐变：`SkPath`/`SkGradientShader` 源码未拉取；当前路径运算与渐变
   实现以既有测试固定，待源码级复核。
