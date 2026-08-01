import { defineConfig, globalIgnores } from "eslint/config";
import nextVitals from "eslint-config-next/core-web-vitals";
import nextTs from "eslint-config-next/typescript";

const eslintConfig = defineConfig([
  ...nextVitals,
  ...nextTs,
  {
    rules: {
      // TanStack Form's typed field API intentionally accepts render children
      // through a `children` prop (the project form contract uses this shape).
      "react/no-children-prop": "off",
      // These React Compiler rules reject established SPA patterns in this
      // project: Query/URL snapshot-to-draft synchronization, in-memory lease
      // token refs, and memoized TanStack/Recharts data. Keep the standard
      // Hooks rules enabled while compiler adoption remains out of scope.
      "react-hooks/immutability": "off",
      "react-hooks/preserve-manual-memoization": "off",
      "react-hooks/purity": "off",
      "react-hooks/refs": "off",
      "react-hooks/set-state-in-effect": "off",
    },
  },
  // Override default ignores of eslint-config-next.
  globalIgnores([
    // Default ignores of eslint-config-next:
    ".next/**",
    "out/**",
    "build/**",
    "next-env.d.ts",
  ]),
]);

export default eslintConfig;
