import type { ComponentPropsWithoutRef } from "react";
import { useAtomValue } from "jotai";
import { readingFullWidthTablesAtom } from "@/stores/readingMode";
import { markdownTableClass } from "./readingWidth";

export function ReadingTable(props: ComponentPropsWithoutRef<"table">) {
  const fullWidth = useAtomValue(readingFullWidthTablesAtom);
  return <table className={markdownTableClass(fullWidth)} {...props} />;
}
