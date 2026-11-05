import { PRODUCT_NAME } from "@/lib/constants";

const integrationOptions = [
  {
    title: "Integrations",
    properties: [
      {
        id: "integrations.terminal.disabled",
        title: "Terminal",
        description: `Add ${PRODUCT_NAME} to Terminal.app`,

        type: "boolean",
        default: false,
        inverted: true,
      },
      {
        id: "integrations.hyper.disabled",
        title: "Hyper",
        description: `Add ${PRODUCT_NAME} to Hyper`,
        type: "boolean",
        default: false,
        inverted: true,
      },
      {
        id: "integrations.vscode.disabled",
        title: "VS Code Terminal",
        description: `Add ${PRODUCT_NAME} to VS Code's built-in terminal`,
        type: "boolean",
        default: false,
        inverted: true,
      },
      {
        id: "integrations.iterm.disabled",
        title: "iTerm 2",
        description: `Add ${PRODUCT_NAME} to iTerm 2`,
        type: "boolean",
        default: false,
        inverted: true,
      },
    ],
  },
];

export default integrationOptions;
