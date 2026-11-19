import { Box, Compass, Globe, House, NotebookPen, Users, type LucideIcon } from "lucide-react";
import type { PageId } from "../../data/types";

export interface NavItem {
  id: PageId;
  label: string;
  icon: LucideIcon;
}

/**
 * The sidebar's pages, in order. ⌘1–⌘6 (Ctrl on Windows and Linux) follow this order too.
 * Servers sit with Worlds and Screenshots with the Journal (as tabs); Skins and Cloud sync are in
 * the account menu at the foot, beside Settings (⌘,).
 */
export const NAV_ITEMS: NavItem[] = [
  { id: "home", label: "Home", icon: House },
  { id: "instances", label: "Instances", icon: Box },
  { id: "worlds", label: "Worlds", icon: Globe },
  { id: "discover", label: "Discover", icon: Compass },
  { id: "journal", label: "Journal", icon: NotebookPen },
  { id: "friends", label: "Friends", icon: Users },
];

/** The sidebar entry a page belongs to. Skins and Cloud sync belong to the account button. */
export function navParent(page: PageId): PageId {
  switch (page) {
    case "project":
      return "discover";
    case "world":
    case "servers":
    case "hosted":
      return "worlds";
    case "screenshots":
      return "journal";
    default:
      return page;
  }
}
