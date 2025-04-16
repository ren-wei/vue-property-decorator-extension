# vue property decorator extension

![Visual Studio Marketplace Version](https://img.shields.io/visual-studio-marketplace/v/ren-wei.vue-property-decorator-extension)
![Visual Studio Marketplace Downloads](https://img.shields.io/visual-studio-marketplace/d/ren-wei.vue-property-decorator-extension)
![Visual Studio Marketplace Installs](https://img.shields.io/visual-studio-marketplace/i/ren-wei.vue-property-decorator-extension)
![LICENSE](https://img.shields.io/badge/license-MIT-green)

这是一个用于支持 `vue2 + typescript + decorator` (`vue-property-decorator`) 项目的高性能 vscode 插件。

此插件采用 LSP 架构，语言功能后端采用 rust 编写，ts 相关语言功能底层由 `tsserver` 完成。

> 注意: 此插件在 windows 下启动速度较慢，建议在 WSL 下使用。

## 功能

* 支持 template 中的表达式语言功能，包括，悬浮提示，自动补全，跳转到定义，语义着色，语法校验等

* 支持 template 标签名悬浮提示组件描述，跳转到组件定义

* 支持 template 标签属性自动补全组件属性，跳转到组件属性定义

* 完全支持 script 标签的 ts 语言功能

## 优势

* 响应速度非常快，几乎相当于原生 ts 的响应速度

* 内存占用极低

* 将组件属性与标签属性相关联，在模版上编辑标签属性时，自动根据对应的组件的属性进行补全提示

## 安装插件

在 `vscode` 扩展中搜索 `vue-property-decorator-extension`，然后点击安装即可。

## 开始

在 vue2 + ts + decorator 项目启用此插件即可。由于此插件专用于 vue2 装饰器语法的项目，所以建议仅在此类型的项目单独启用此插件。

避免相互影响，在此插件启用时，请禁用 `Vetur(octref.vetur)`、`Volar(vue.volar)` 等插件。

## 与其他 vue 插件对比

`Vetur` 和 `Volar` 也支持 vue2 装饰器格式的语法，但是它们对此模式的支持并不是很好。

* `Vetur` 在此模式下，不支持 template 上的表达式的语言功能

* `Volar` 在此模式下，注册组件的属性和 template 上的标签属性没有关联语言功能；在大型项目下性能很差，甚至无法使用

## Issues

如果您在使用过程中遇到问题，您可以创建一个 [Issues](https://github.com/ren-wei/vue-property-decorator-extension/issues) ，我们会尽快为您解决。

