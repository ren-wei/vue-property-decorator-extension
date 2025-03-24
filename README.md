# vue property decorator extension

![Visual Studio Marketplace Version](https://img.shields.io/visual-studio-marketplace/v/ren-wei.vue-property-decorator-extension)
![Visual Studio Marketplace Downloads](https://img.shields.io/visual-studio-marketplace/d/ren-wei.vue-property-decorator-extension)
![Visual Studio Marketplace Installs](https://img.shields.io/visual-studio-marketplace/i/ren-wei.vue-property-decorator-extension)
![LICENSE](https://img.shields.io/badge/license-MIT-green)

[中文文档](https://github.com/ren-wei/vue-property-decorator-extension/blob/master/README-zh-CN.md)

This is a high-performance LSP extension of vscode that is support `vue2 + typescript + decorator` ( `vue-property-decorator`).

This extension adopts the LSP architecture, the backend of the language features is written in rust, and the underlying of the typescript language features is done by `tsserver`.

> Note: This extension is currently only supported on Linux and Mac platforms，Windows will be supported in the future and is currently recommended for use under WSL.

## Features

* Language features that support expressions in template tag, including Hover, Completion, Goto Definition, Semantic Tokens, Diagnostics, and more.

* The template tag name can be used to overhead the component description, the tag attribute will automatically complete the component attribute, and the tag name can jump to the component definition.

* The TS language feature of script tag is fully supported.

## Advantage

* The response speed is very fast, almost equivalent to the response speed of native TS.

* Extremely low memory footprint.

* When you associate a component attribute with a tag attribute, when you edit the tag attribute on the template, the completion prompt is automatically performed based on the attribute of the corresponding component.

## Installed extensions

Search for `vue-property-decorator-extension` in the `vscode` extension, and then click to install.

## Getting Started

Enable this extension in your vue2 + ts + decorator project. Since this plugin is dedicated to projects with vue2 decorator syntax, it is recommended that you enable this plugin separately only for projects of this type.

To avoid interfering with each other, please disable extension like `Vetur(octref.vetur)`, `Volar(vue.volar)` while this plugin is enabled.

## Compare to other Vue extensions

`Vetur` and `Volar` also support the syntax of the vue2 decorating format, but they don't support this pattern very well.

* `Vetur` does not support the language features of expressions on the template in this mode.

* `Volar` In this mode, there is no associated language feature between the properties of the registered component and the label properties on the template; Poor performance under large projects and even unusable

## Issues

If you encounter any problem in use, you can create a [Issues](https://github.com/ren-wei/vue-property-decorator-extension/issues) and we will solve it for you as soon as possible.

