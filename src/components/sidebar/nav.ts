import { Box, Cloud, Compass, Globe, House, Images, NotebookPen, Shirt, Users, type LucideIcon } from "lucide-react";
import type { PageId } from "../../data/types";

export interface NavItem {
  id: PageId;
  label: string;
  icon: LucideIcon;
}

/** The sidebar's pages, in order. ⌘1–⌘9 (Ctrl on Windows and Linux) follow this order too. Settings sits at the foot (⌘,). */
export const NAV_ITEMS: NavItem[] = [
  { id: "home", label: "Home", icon: House },
  { id: "instances", label: "Instances", icon: Box },
  { id: "worlds", label: "Worlds", icon: Globe },
  { id: "discover", label: "Discover", icon: Compass },
  { id: "skins", label: "Skins", icon: Shirt },
  { id: "screenshots", label: "Screenshots", icon: Images },
  { id: "journal", label: "Journal", icon: NotebookPen },
  { id: "friends", label: "Friends", icon: Users },
  { id: "sync", label: "Cloud sync", icon: Cloud },
];

/** The sidebar entry a page belongs to: a project's page to Discover, a world's to Worlds. */
export function navParent(page: PageId): PageId {
  return page === "project" ? "discover" : page === "world" ? "worlds" : page;
}
