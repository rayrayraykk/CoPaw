import type { HarvestThemeConfig } from "../types";

export const HARVEST_THEMES: Record<string, HarvestThemeConfig> = {
  apple: {
    id: "apple",
    name: "Apple Style",
    description: "Clean, minimalist design inspired by Apple's aesthetic",
    colors: {
      primary: "#007AFF",
      secondary: "#5856D6",
      background: "#FFFFFF",
      text: "#1D1D1F",
      accent: "#FF3B30",
    },
    fonts: {
      heading: "SF Pro Display, -apple-system, BlinkMacSystemFont, sans-serif",
      body: "SF Pro Text, -apple-system, BlinkMacSystemFont, sans-serif",
    },
    preview: "/themes/apple-preview.png",
  },
  deepblue: {
    id: "deepblue",
    name: "Deep Blue",
    description: "Professional ocean-inspired design with deep blue tones",
    colors: {
      primary: "#0A4D68",
      secondary: "#088395",
      background: "#05161A",
      text: "#E8F0F2",
      accent: "#00D9FF",
    },
    fonts: {
      heading: "Inter, sans-serif",
      body: "Inter, sans-serif",
    },
    preview: "/themes/deepblue-preview.png",
  },
  autumn: {
    id: "autumn",
    name: "Autumn Warmth",
    description: "Warm, cozy design with autumn-inspired colors",
    colors: {
      primary: "#D4691A",
      secondary: "#8B4513",
      background: "#FFF8E7",
      text: "#3E2723",
      accent: "#FF6F3C",
    },
    fonts: {
      heading: "Merriweather, serif",
      body: "Merriweather, serif",
    },
    preview: "/themes/autumn-preview.png",
  },
  minimal: {
    id: "minimal",
    name: "Minimal Black & White",
    description: "Ultra-minimal monochrome design for focused reading",
    colors: {
      primary: "#000000",
      secondary: "#333333",
      background: "#FFFFFF",
      text: "#000000",
      accent: "#666666",
    },
    fonts: {
      heading: "Helvetica Neue, sans-serif",
      body: "Georgia, serif",
    },
    preview: "/themes/minimal-preview.png",
  },
  magazine: {
    id: "magazine",
    name: "Modern Magazine",
    description: "Bold, colorful design inspired by fashion magazines",
    colors: {
      primary: "#FF1744",
      secondary: "#7C4DFF",
      background: "#FAFAFA",
      text: "#212121",
      accent: "#FF6D00",
    },
    fonts: {
      heading: "Playfair Display, serif",
      body: "Lato, sans-serif",
    },
    preview: "/themes/magazine-preview.png",
  },
  newspaper: {
    id: "newspaper",
    name: "Classic Newspaper",
    description: "Traditional newspaper layout with serif fonts",
    colors: {
      primary: "#1A1A1A",
      secondary: "#4A4A4A",
      background: "#F5F5DC",
      text: "#000000",
      accent: "#8B0000",
    },
    fonts: {
      heading: "Times New Roman, serif",
      body: "Times New Roman, serif",
    },
    preview: "/themes/newspaper-preview.png",
  },
};

export const getThemeConfig = (
  themeId: string,
): HarvestThemeConfig | undefined => {
  return HARVEST_THEMES[themeId];
};

export const getDefaultTheme = (): HarvestThemeConfig => {
  return HARVEST_THEMES.apple;
};
