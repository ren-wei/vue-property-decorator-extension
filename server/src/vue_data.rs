use std::sync::Arc;

use html_languageservice::language_facts::data_provider::{
    HTMLDataProvider, HTMLDataProviderContent, IHTMLDataProvider,
};

#[derive(Clone)]
pub struct VueDataProvider(Arc<HTMLDataProvider>);

impl VueDataProvider {
    pub fn new() -> VueDataProvider {
        VueDataProvider(Arc::new(HTMLDataProvider::new(
            "vue".to_string(),
            serde_json::from_str(VUE_DATA).unwrap(),
            true,
        )))
    }
}

impl IHTMLDataProvider for VueDataProvider {
    fn get_id(&self) -> &str {
        self.0.get_id()
    }

    fn is_applicable(&self, language_id: &str) -> bool {
        self.0.is_applicable(language_id)
    }

    fn provide_tags(&self) -> &Vec<html_languageservice::html_data::ITagData> {
        self.0.provide_tags()
    }

    fn provide_attributes(
        &self,
        tag: &str,
        content: &HTMLDataProviderContent<'_>,
    ) -> Vec<&html_languageservice::html_data::IAttributeData> {
        self.0.provide_attributes(tag, content)
    }

    fn provide_values(
        &self,
        tag: &str,
        attribute: &str,
    ) -> Vec<&html_languageservice::html_data::IValueData> {
        self.0.provide_values(tag, attribute)
    }
}

static VUE_DATA: &str = r##"{
    "version": 1,
    "tags": [
        {
            "name": "component",
            "attributes": [
                {
                    "name": "is",
                    "description": "the actual component to render"
                },
                {
                    "name": "inline-template",
                    "description": "treat inner content as its template rather than distributed content"
                }
            ],
            "references": [
                {
                    "name": "API Reference",
                    "url": "https://vuejs.org/v2/api/#component"
                }
            ]
        },
        {
            "name": "transition",
            "attributes": [
                {
                    "name": "name",
                    "description": "Used to automatically generate transition CSS class names. Default: \"v\"",
                    "references": [
                        {
                            "name": "API Reference",
                            "url": "https://vuejs.org/v2/api/#name"
                        }
                    ]
                },
                {
                    "name": "appear",
                    "description": "Whether to apply transition on initial render. Default: false",
                    "references": [
                        {
                            "name": "API Reference",
                            "url": "https://vuejs.org/v2/api/#appear"
                        }
                    ]
                },
                {
                    "name": "css",
                    "description": "Whether to apply CSS transition classes. Defaults: true. If set to false, will only trigger JavaScript hooks registered via component events.",
                    "references": [
                        {
                            "name": "API Reference",
                            "url": "https://vuejs.org/v2/api/#css"
                        }
                    ]
                },
                {
                    "name": "type",
                    "description": "The event, \"transition\" or \"animation\", to determine end timing. Default: the type that has a longer duration.",
                    "references": [
                        {
                            "name": "API Reference",
                            "url": "https://vuejs.org/v2/api/#type"
                        }
                    ]
                },
                {
                    "name": "mode",
                    "description": "Controls the timing sequence of leaving/entering transitions. Available modes are \"out-in\" and \"in-out\"; Defaults to simultaneous.",
                    "references": [
                        {
                            "name": "API Reference",
                            "url": "https://vuejs.org/v2/api/#mode"
                        }
                    ]
                },
                {
                    "name": "enter-class"
                },
                {
                    "name": "leave-class"
                },
                {
                    "name": "appear-class"
                },
                {
                    "name": "enter-to-class"
                },
                {
                    "name": "leave-to-class"
                },
                {
                    "name": "appear-to-class"
                },
                {
                    "name": "enter-active-class"
                },
                {
                    "name": "leave-active-class"
                },
                {
                    "name": "appear-active-class"
                }
            ],
            "references": [
                {
                    "name": "API Reference",
                    "url": "https://vuejs.org/v2/api/#transition"
                }
            ]
        },
        {
            "name": "transition-group",
            "attributes": [
                {
                    "name": "name",
                    "description": "Used to automatically generate transition CSS class names. Default: \"v\"",
                    "references": [
                        {
                            "name": "API Reference",
                            "url": "https://vuejs.org/v2/api/#name"
                        }
                    ]
                },
                {
                    "name": "appear",
                    "description": "Whether to apply transition on initial render. Default: false",
                    "references": [
                        {
                            "name": "API Reference",
                            "url": "https://vuejs.org/v2/api/#appear"
                        }
                    ]
                },
                {
                    "name": "css",
                    "description": "Whether to apply CSS transition classes. Defaults: true. If set to false, will only trigger JavaScript hooks registered via component events.",
                    "references": [
                        {
                            "name": "API Reference",
                            "url": "https://vuejs.org/v2/api/#css"
                        }
                    ]
                },
                {
                    "name": "type",
                    "description": "The event, \"transition\" or \"animation\", to determine end timing. Default: the type that has a longer duration.",
                    "references": [
                        {
                            "name": "API Reference",
                            "url": "https://vuejs.org/v2/api/#type"
                        }
                    ]
                },
                {
                    "name": "mode",
                    "description": "Controls the timing sequence of leaving/entering transitions. Available modes are \"out-in\" and \"in-out\"; Defaults to simultaneous.",
                    "references": [
                        {
                            "name": "API Reference",
                            "url": "https://vuejs.org/v2/api/#mode"
                        }
                    ]
                },
                {
                    "name": "enter-class"
                },
                {
                    "name": "leave-class"
                },
                {
                    "name": "appear-class"
                },
                {
                    "name": "enter-to-class"
                },
                {
                    "name": "leave-to-class"
                },
                {
                    "name": "appear-to-class"
                },
                {
                    "name": "enter-active-class"
                },
                {
                    "name": "leave-active-class"
                },
                {
                    "name": "appear-active-class"
                },
                {
                    "name": "tag"
                },
                {
                    "name": "move-class"
                }
            ],
            "references": [
                {
                    "name": "API Reference",
                    "url": "https://vuejs.org/v2/api/#transition-group"
                }
            ]
        },
        {
            "name": "keep-alive",
            "attributes": [
                {
                    "name": "include"
                },
                {
                    "name": "exclude"
                }
            ],
            "references": [
                {
                    "name": "API Reference",
                    "url": "https://vuejs.org/v2/api/#keep-alive"
                }
            ]
        },
        {
            "name": "slot",
            "attributes": [
                {
                    "name": "name",
                    "description": "Used for named slot"
                }
            ],
            "references": [
                {
                    "name": "API Reference",
                    "url": "https://vuejs.org/v2/api/#slot"
                }
            ]
        },
        {
            "name": "template",
            "attributes": [
                {
                    "name": "scope",
                    "description": "(deprecated) a temporary variable that holds the props object passed from the child"
                },
                {
                    "name": "slot",
                    "description": "the name of scoped slot"
                }
            ]
        },
        {
            "name": "router-link",
            "attributes": [
                {
                    "name": "to",
                    "description": "The target route of the link. It can be either a string or a location descriptor object.",
                    "references": [
                        {
                            "name": "API Reference",
                            "url": "https://router.vuejs.org/api/#to"
                        }
                    ]
                },
                {
                    "name": "replace",
                    "description": "Setting replace prop will call `router.replace()` instead of `router.push()` when clicked, so the navigation will not leave a history record.",
                    "references": [
                        {
                            "name": "API Reference",
                            "url": "https://router.vuejs.org/api/#replace"
                        }
                    ]
                },
                {
                    "name": "append",
                    "description": "Setting append prop always appends the relative path to the current path. For example, assuming we are navigating from /a to a relative link b, without append we will end up at /b, but with append we will end up at /a/b.",
                    "references": [
                        {
                            "name": "API Reference",
                            "url": "https://router.vuejs.org/api/#append"
                        }
                    ]
                },
                {
                    "name": "tag",
                    "description": "Specify which tag to render to, and it will still listen to click events for navigation.",
                    "references": [
                        {
                            "name": "API Reference",
                            "url": "https://router.vuejs.org/api/#tag"
                        }
                    ]
                },
                {
                    "name": "active-class",
                    "description": "Configure the active CSS class applied when the link is active.",
                    "references": [
                        {
                            "name": "API Reference",
                            "url": "https://router.vuejs.org/api/#active-class"
                        }
                    ]
                },
                {
                    "name": "exact",
                    "description": "Force the link into \"exact match mode\".",
                    "references": [
                        {
                            "name": "API Reference",
                            "url": "https://router.vuejs.org/api/#exact"
                        }
                    ]
                },
                {
                    "name": "event",
                    "description": "Specify the event(s) that can trigger the link navigation.",
                    "references": [
                        {
                            "name": "API Reference",
                            "url": "https://router.vuejs.org/api/#event"
                        }
                    ]
                },
                {
                    "name": "exact-active-class",
                    "description": "Configure the active CSS class applied when the link is active with exact match.",
                    "references": [
                        {
                            "name": "API Reference",
                            "url": "https://router.vuejs.org/api/#exact-active-class"
                        }
                    ]
                },
                {
                    "name": "aria-current-value",
                    "description": "Configure the value of `aria-current` when the link is active with exact match. It must be one of the [allowed values for `aria-current`](https://www.w3.org/TR/wai-aria-1.2/#aria-current) in the ARIA spec. In most cases, the default of `page` should be the best fit.",
                    "references": [
                        {
                            "name": "API Reference",
                            "url": "https://router.vuejs.org/api/#aria-current-value"
                        }
                    ]
                }
            ],
            "references": [
                {
                    "name": "API Reference",
                    "url": "https://router.vuejs.org/api/#router-link"
                }
            ]
        },
        {
            "name": "router-view",
            "attributes": [
                {
                    "name": "name",
                    "description": "When a `<router-view>` has a name, it will render the component with the corresponding name in the matched route record's components option.",
                    "references": [
                        {
                            "name": "API Reference",
                            "url": "https://router.vuejs.org/api/#to"
                        }
                    ]
                }
            ],
            "references": [
                {
                    "name": "API Reference",
                    "url": "https://router.vuejs.org/api/#router-link"
                }
            ]
        }
    ],
    "globalAttributes": [
        {
            "name": "v-text",
            "description": "Updates the element’s `textContent`.",
            "references": [
                {
                    "name": "API Reference",
                    "url": "https://vuejs.org/v2/api/#v-text"
                }
            ]
        },
        {
            "name": "v-html",
            "description": "Updates the element’s `innerHTML`. XSS prone.",
            "references": [
                {
                    "name": "API Reference",
                    "url": "https://vuejs.org/v2/api/#v-html"
                }
            ]
        },
        {
            "name": "v-show",
            "description": "Toggle’s the element’s `display` CSS property based on the truthy-ness of the expression value.",
            "references": [
                {
                    "name": "API Reference",
                    "url": "https://vuejs.org/v2/api/#v-show"
                }
            ]
        },
        {
            "name": "v-if",
            "description": "Conditionally renders the element based on the truthy-ness of the expression value.",
            "references": [
                {
                    "name": "API Reference",
                    "url": "https://vuejs.org/v2/api/#v-if"
                }
            ]
        },
        {
            "name": "v-else",
            "description": "Denotes the “else block” for `v-if` or a `v-if`/`v-else-if` chain.",
            "references": [
                {
                    "name": "API Reference",
                    "url": "https://vuejs.org/v2/api/#v-else"
                }
            ]
        },
        {
            "name": "v-else-if",
            "description": "Denotes the “else if block” for `v-if`. Can be chained.",
            "references": [
                {
                    "name": "API Reference",
                    "url": "https://vuejs.org/v2/api/#v-else-if"
                }
            ]
        },
        {
            "name": "v-for",
            "description": "Renders the element or template block multiple times based on the source data.",
            "references": [
                {
                    "name": "API Reference",
                    "url": "https://vuejs.org/v2/api/#v-for"
                }
            ]
        },
        {
            "name": "v-on",
            "description": "Attaches an event listener to the element.",
            "references": [
                {
                    "name": "API Reference",
                    "url": "https://vuejs.org/v2/api/#v-on"
                }
            ]
        },
        {
            "name": "v-bind",
            "description": "Dynamically binds one or more attributes, or a component prop to an expression.",
            "references": [
                {
                    "name": "API Reference",
                    "url": "https://vuejs.org/v2/api/#v-bind"
                }
            ]
        },
        {
            "name": "v-model",
            "description": "Creates a two-way binding on a form input element or a component.",
            "references": [
                {
                    "name": "API Reference",
                    "url": "https://vuejs.org/v2/api/#v-model"
                }
            ]
        },
        {
            "name": "v-pre",
            "description": "Skips compilation for this element and all its children.",
            "references": [
                {
                    "name": "API Reference",
                    "url": "https://vuejs.org/v2/api/#v-pre"
                }
            ]
        },
        {
            "name": "v-cloak",
            "description": "Indicates Vue instance for this element has NOT finished compilation.",
            "references": [
                {
                    "name": "API Reference",
                    "url": "https://vuejs.org/v2/api/#v-cloak"
                }
            ]
        },
        {
            "name": "v-once",
            "description": "Render the element and component once only.",
            "references": [
                {
                    "name": "API Reference",
                    "url": "https://vuejs.org/v2/api/#v-once"
                }
            ]
        },
        {
            "name": "key",
            "description": "Hint at VNodes identity for VDom diffing, e.g. list rendering",
            "references": [
                {
                    "name": "API Reference",
                    "url": "https://vuejs.org/v2/api/#key"
                }
            ]
        },
        {
            "name": "ref",
            "description": "Register a reference to an element or a child component.",
            "references": [
                {
                    "name": "API Reference",
                    "url": "https://vuejs.org/v2/api/#ref"
                }
            ]
        },
        {
            "name": "slot",
            "description": "Used on content inserted into child components to indicate which named slot the content belongs to.",
            "references": [
                {
                    "name": "API Reference",
                    "url": "https://vuejs.org/v2/api/#slot"
                }
            ]
        },
        {
            "name": "slot-scope",
            "description": "the name of a temporary variable that holds the props object passed from the child",
            "references": [
                {
                    "name": "API Reference",
                    "url": "https://vuejs.org/v2/api/#slot-scope"
                }
            ]
        }
    ]
}"##;
