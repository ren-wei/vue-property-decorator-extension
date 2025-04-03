/* eslint-disable @typescript-eslint/naming-convention */
interface Attribute {
    label: string;
    type?: string;
    documentation?: string;
    referenceName?: string;
    referenceUrl?: string;
}
class HTMLTagSpecification {
    // eslint-disable-next-line no-useless-constructor
    constructor(public documentation: string, public referenceName: string, public referenceUrl: string, public attributes: Attribute[] = []) {}
}

function genAttribute(label: string, type?: string, documentation?: string, referenceName?: string, referenceUrl?: string): Attribute {
    return { label, type, documentation, referenceName, referenceUrl };
}

function getAttribute(label: string, type: string | undefined, documentation: string) {
    const linkedDocumentation = documentation;
    return genAttribute(label, type, linkedDocumentation, "API Reference", `https://vuejs.org/v2/api/#${label}`);
}

const vueDirectives = [
    getAttribute("v-text", undefined, "Updates the element’s `textContent`."),
    getAttribute("v-html", undefined, "Updates the element’s `innerHTML`. XSS prone."),
    getAttribute(
        "v-show",
        undefined,
        "Toggle’s the element’s `display` CSS property based on the truthy-ness of the expression value."
    ),
    getAttribute(
        "v-if",
        undefined,
        "Conditionally renders the element based on the truthy-ness of the expression value."
    ),
    getAttribute("v-else", "v", "Denotes the “else block” for `v-if` or a `v-if`/`v-else-if` chain."),
    getAttribute("v-else-if", undefined, "Denotes the “else if block” for `v-if`. Can be chained."),
    getAttribute("v-for", undefined, "Renders the element or template block multiple times based on the source data."),
    getAttribute("v-on", undefined, "Attaches an event listener to the element."),
    getAttribute("v-bind", undefined, "Dynamically binds one or more attributes, or a component prop to an expression."),
    getAttribute("v-model", undefined, "Creates a two-way binding on a form input element or a component."),
    getAttribute("v-pre", "v", "Skips compilation for this element and all its children."),
    getAttribute("v-cloak", "v", "Indicates Vue instance for this element has NOT finished compilation."),
    getAttribute("v-once", "v", "Render the element and component once only."),
    getAttribute("key", undefined, "Hint at VNodes identity for VDom diffing, e.g. list rendering"),
    getAttribute("ref", undefined, "Register a reference to an element or a child component."),
    getAttribute(
        "slot",
        undefined,
        "Used on content inserted into child components to indicate which named slot the content belongs to."
    ),
    getAttribute(
        "slot-scope",
        undefined,
        "the name of a temporary variable that holds the props object passed from the child"
    ),
];

const transitionProps = [
    getAttribute("name", undefined, 'Used to automatically generate transition CSS class names. Default: "v"'),
    getAttribute("appear", "b", "Whether to apply transition on initial render. Default: false"),
    getAttribute(
        "css",
        "b",
        "Whether to apply CSS transition classes. Defaults: true. If set to false, will only trigger JavaScript hooks registered via component events."
    ),
    getAttribute(
        "type",
        "transType",
        'The event, "transition" or "animation", to determine end timing. Default: the type that has a longer duration.'
    ),
    getAttribute(
        "mode",
        "transMode",
        'Controls the timing sequence of leaving/entering transitions. Available modes are "out-in" and "in-out"; Defaults to simultaneous.'
    ),
].concat(
    [
        "enter-class",
        "leave-class",
        "appear-class",
        "enter-to-class",
        "leave-to-class",
        "appear-to-class",
        "enter-active-class",
        "leave-active-class",
        "appear-active-class",
    ].map(t => genAttribute(t))
);

function genTag(tag: string, doc: string, attributes: Attribute[]) {
    return new HTMLTagSpecification(doc, "API Reference", `https://vuejs.org/v2/api/#${tag}`, attributes);
}

const vueTags = {
    component: genTag(
        "component",
        "A meta component for rendering dynamic components. The actual component to render is determined by the `is` prop.",
        [
            genAttribute("is", undefined, "the actual component to render"),
            genAttribute("inline-template", "v", "treat inner content as its template rather than distributed content"),
        ]
    ),
    transition: genTag(
        "transition",
        "<transition> serves as transition effects for single element/component. It applies the transition behavior to the wrapped content inside.",
        transitionProps
    ),
    // eslint-disable-next-line @typescript-eslint/naming-convention
    "transition-group": genTag(
        "transition-group",
        "transition group serves as transition effects for multiple elements/components. It renders a <span> by default and can render user specified element via `tag` attribute.",
        transitionProps.concat(genAttribute("tag"), genAttribute("move-class"))
    ),
    // eslint-disable-next-line @typescript-eslint/naming-convention
    "keep-alive": genTag(
        "keep-alive",
        "When wrapped around a dynamic component, <keep-alive> caches the inactive component instances without destroying them.",
        ["include", "exclude"].map(t => genAttribute(t))
    ),
    slot: genTag(
        "slot",
        "<slot> serve as content distribution outlets in component templates. <slot> itself will be replaced.",
        [genAttribute("name", undefined, "Used for named slot")]
    ),
    template: new HTMLTagSpecification(
        "The template element is used to declare fragments of HTML that can be cloned and inserted in the document by script.",
        "",
        "",
        [
            genAttribute(
                "scope",
                undefined,
                "(deprecated) a temporary variable that holds the props object passed from the child"
            ),
            genAttribute("slot", undefined, "the name of scoped slot"),
        ]
    ),
    "router-link": new HTMLTagSpecification(
        "Link to navigate user. The target location is specified with the to prop.",
        "API Reference",
        "https://router.vuejs.org/api/#router-link",
        [
            genAttribute(
                "to",
                undefined,
                "The target route of the link. It can be either a string or a location descriptor object.",
                "API Reference",
                "https://router.vuejs.org/api/#to"
            ),
            genAttribute(
                "replace",
                undefined,
                "Setting replace prop will call `router.replace()` instead of `router.push()` when clicked, so the navigation will not leave a history record.",
                "API Reference",
                "https://router.vuejs.org/api/#replace",
            ),
            genAttribute(
                "append",
                "v",
                "Setting append prop always appends the relative path to the current path. For example, assuming we are navigating from /a to a relative link b, without append we will end up at /b, but with append we will end up at /a/b.",
                "API Reference",
                "https://router.vuejs.org/api/#append",
            ),
            genAttribute(
                "tag",
                undefined,
                "Specify which tag to render to, and it will still listen to click events for navigation.",
                "API Reference",
                "https://router.vuejs.org/api/#tag",
            ),
            genAttribute(
                "active-class",
                undefined,
                "Configure the active CSS class applied when the link is active.",
                "API Reference",
                "https://router.vuejs.org/api/#active-class",
            ),
            genAttribute(
                "exact",
                "v",
                'Force the link into "exact match mode".',
                "API Reference",
                "https://router.vuejs.org/api/#exact"
            ),
            genAttribute(
                "event",
                undefined,
                "Specify the event(s) that can trigger the link navigation.",
                "API Reference",
                "https://router.vuejs.org/api/#event",
            ),
            genAttribute(
                "exact-active-class",
                undefined,
                "Configure the active CSS class applied when the link is active with exact match.",
                "API Reference",
                "https://router.vuejs.org/api/#exact-active-class",
            ),
            genAttribute(
                "aria-current-value",
                "ariaCurrentType",
                "Configure the value of `aria-current` when the link is active with exact match. It must be one of the [allowed values for `aria-current`](https://www.w3.org/TR/wai-aria-1.2/#aria-current) in the ARIA spec. In most cases, the default of `page` should be the best fit.",
                "API Reference",
                "https://router.vuejs.org/api/#aria-current-value",
            ),
        ]
    ),
    "router-view": new HTMLTagSpecification(
        "A functional component that renders the matched component for the given path. Components rendered in <router-view> can also contain its own <router-view>, which will render components for nested paths.",
        "API Reference",
        "https://router.vuejs.org/api/#router-link",
        [
            genAttribute(
                "name",
                undefined,
                "When a `<router-view>` has a name, it will render the component with the corresponding name in the matched route record's components option.",
                "API Reference",
                "https://router.vuejs.org/api/#to",
            ),
        ]
    ),
};

const valueSets = {
    transMode: ["out-in", "in-out"],
    transType: ["transition", "animation"],
    b: ["true", "false"],
};

interface HTMLDataV1 {
    version: number;
    tags: ITagData[];
    globalAttributes: IAttributeData[];
    valueSets?: IValueSet[];
}

interface ITagData {
    name: string;
    description?: string;
    attributes: IAttributeData[];
    references?: IReference[];
    void?: boolean;
}

interface IAttributeData {
    name: string;
    description?: string;
    valueSet?: string;
    values?: IValueData[];
    references?: IReference[];
}

interface IReference {
    name: string;
    url: string;
}

interface IValueSet {
    name: string,
    values: IValueData[],
}

interface IValueData {
    name: string;
    description?: string;
    references?: IReference[];
}

const data: HTMLDataV1 = {
    version: 1,
    tags: [],
    globalAttributes: [],
};

for (const name in vueTags) {
    const value = vueTags[name as keyof typeof vueTags];
    data.tags.push({
        name,
        attributes: value.attributes.map(attr => ({
            name: attr.label,
            description: attr.documentation,
            references: attr.referenceName ? [{
                name: attr.referenceName,
                url: attr.referenceUrl as string,
            }] : undefined,
        })),
        references: value.referenceName ? [{
            name: value.referenceName,
            url: value.referenceUrl,
        }] : undefined,
    });
}

for (const item of vueDirectives) {
    data.globalAttributes.push({
        name: item.label,
        description: item.documentation,
        references: item.referenceName ? [{
            name: item.referenceName,
            url: item.referenceUrl as string,
        }] : undefined,
    });
}
// eslint-disable-next-line no-console
console.log(JSON.stringify(data, null, 4));
