/** @type {import('eslint').Linter.Config} */
// eslint-disable-next-line no-undef
module.exports = {
    "root": true,
    "parser": "@typescript-eslint/parser",
    "parserOptions": {
        "ecmaVersion": 6,
        "sourceType": "module",
    },
    "plugins": [
        "@typescript-eslint",
    ],
    "extends": [
        "plugin:strict-typescript/recommend",
    ],
    "ignorePatterns": [
        "out",
        "dist",
        "**/*.d.ts",
    ],
};
