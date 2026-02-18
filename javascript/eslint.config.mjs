// ESLint flat config for Multifrost JavaScript
// Run with: npm run lint

import tseslint from "typescript-eslint";

export default tseslint.config(
    {
        ignores: ["node_modules/", "dist/", "*.js"],
    },
    ...tseslint.configs.recommended,
    {
        rules: {
            "@typescript-eslint/no-unused-vars": ["error", { argsIgnorePattern: "^_" }],
            "@typescript-eslint/explicit-function-return-type": "off",
            "@typescript-eslint/no-explicit-any": "warn",
            "no-console": "off",
        },
    }
);
