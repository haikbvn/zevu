import { PRODUCT_NAME } from "@/lib/constants";

const generalPreferences = [
  {
    title: "Startup",
    properties: [
      {
        id: "app.launchOnStartup",
        title: "Launch on Start",
        description: `Start ${PRODUCT_NAME} automatically whenever you restart your computer.`,
        type: "boolean",
        default: true,
        popular: false,
      },
      {
        id: "app.hideMenubarIcon",
        title: "Display Menu Bar icon",
        description: `${PRODUCT_NAME} icon will appear in the Menu Bar while ${PRODUCT_NAME} is running.`,
        type: "boolean",
        default: false,
        inverted: true,
        popular: false,
      },
    ],
  },
];

export default generalPreferences;
