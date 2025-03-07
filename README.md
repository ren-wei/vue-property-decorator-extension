# vue property decorator extension

## 核心设计思想

* 以文件为基本单位

* 将 vue 组件的 template 中的表达式渲染为 render 函数，由 tsserver 提供语言服务

* vue 组件的 template 的标签属性等部分由 html 服务器来提供语言服务

* vue 组件的 script 部分由 tsserver 来提供语言服务

* vue 组件的 style 部分由 css 服务器提供服务

## 无法被 tsserver 自动处理的文件间依赖

* 继承组件的属性变更时，组件需要重新渲染，使用依赖树

## 依赖树

* 每个节点表示一个文件，每个 vue 文件都在其中

* 只记录 vue 组件的依赖，最下层节点必定是 vue 组件或 vue 组件声明文件

* 下层节点最多依赖一个上层节点

## 库组件

库组件位于 node_modules 中，如果一个库被标记为 ui 库，那么从 `node_modules/<库名>/types/*` 获取库的所有组件定义

## 解析和渲染

需要渲染的文件和渲染目标:

* vue 组件 -- 生成渲染文件；获取属性定义；获取插槽定义；获取抛出事件；

* vue 组件声明文件 -- 获取属性定义；获取插槽定义；获取抛出事件

* 被 vue 组件依赖的 ts 文件 -- 获取所有导入导出项

## vue 文件渲染

**解析文档**

输入:
	- document 文档

输出:
	- template 节点
	- script 节点
	- style 节点

**解析脚本**

输入:
	- script 节点

输出:
	- props 所有属性和方法
	- render_insert_offset `render` 函数插入位置
	- extends_component 继承的组件
	- registers 注册的组件

**模版编译**

输入:
	- template 节点

输出:
	- template_compile_result 模版编译结果
	- mapping 映射关系，编译结果与 template 节点中的位置关系
		* 包含编译后的偏移量，编译前的偏移量和长度
		* 长度在编译前后保持不变
		* 映射关闭保证顺序

**等待继承组件**

* 等待继承组件渲染完成

**组合渲染内容**

输入:
	- script 节点的 start_tag_end 和 end_tag_start
	- template_compile_result
	- props
	- 继承组件的 props
	- render_insert_offset
	- source 源码
