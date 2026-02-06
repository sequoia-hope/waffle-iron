import React from "react";
import { Feature } from "../types";

interface FeatureTreeProps {
  features: Feature[];
  selectedId: string | null;
  onSelect: (id: string) => void;
}

const typeIcons: Record<string, string> = {
  box: "\u25A1",       // square
  cylinder: "\u25CB",  // circle
  sphere: "\u25CF",    // filled circle
  extrude: "\u2B06",   // up arrow
  union: "\u222A",     // union
  subtract: "\u2212",  // minus
  intersect: "\u2229", // intersection
};

const styles: Record<string, React.CSSProperties> = {
  container: {
    width: 220,
    minWidth: 220,
    height: "100%",
    background: "#1e1e2f",
    borderRight: "1px solid #333",
    display: "flex",
    flexDirection: "column",
    color: "#ddd",
    fontSize: 13,
    userSelect: "none",
  },
  header: {
    padding: "10px 12px",
    fontWeight: 600,
    fontSize: 14,
    borderBottom: "1px solid #333",
    color: "#aac4e0",
    letterSpacing: 0.5,
  },
  list: {
    flex: 1,
    overflowY: "auto" as const,
    padding: "4px 0",
  },
  item: {
    padding: "6px 12px",
    cursor: "pointer",
    display: "flex",
    alignItems: "center",
    gap: 8,
  },
  itemSelected: {
    background: "#2a4a6b",
  },
  itemSuppressed: {
    opacity: 0.45,
    textDecoration: "line-through" as const,
  },
  empty: {
    padding: "16px 12px",
    color: "#666",
    fontStyle: "italic" as const,
  },
};

const FeatureTree: React.FC<FeatureTreeProps> = ({
  features,
  selectedId,
  onSelect,
}) => {
  return (
    <div style={styles.container}>
      <div style={styles.header}>Feature Tree</div>
      <div style={styles.list}>
        {features.length === 0 && (
          <div style={styles.empty}>No features yet</div>
        )}
        {features.map((f) => {
          const isSelected = f.id === selectedId;
          return (
            <div
              key={f.id}
              onClick={() => onSelect(f.id)}
              style={{
                ...styles.item,
                ...(isSelected ? styles.itemSelected : {}),
                ...(f.suppressed ? styles.itemSuppressed : {}),
              }}
            >
              <span>{typeIcons[f.type] ?? "?"}</span>
              <span>{f.label}</span>
            </div>
          );
        })}
      </div>
    </div>
  );
};

export default FeatureTree;
