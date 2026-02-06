import React from "react";
import { ToolAction } from "../types";

interface ToolbarProps {
  onAction: (action: ToolAction) => void;
}

interface ButtonDef {
  action: ToolAction;
  label: string;
  group: "primitives" | "operations";
}

const buttons: ButtonDef[] = [
  { action: "box", label: "Box", group: "primitives" },
  { action: "cylinder", label: "Cylinder", group: "primitives" },
  { action: "sphere", label: "Sphere", group: "primitives" },
  { action: "extrude", label: "Extrude", group: "primitives" },
  { action: "union", label: "Union", group: "operations" },
  { action: "subtract", label: "Subtract", group: "operations" },
  { action: "intersect", label: "Intersect", group: "operations" },
];

const styles: Record<string, React.CSSProperties> = {
  toolbar: {
    height: 44,
    minHeight: 44,
    background: "#16162a",
    borderBottom: "1px solid #333",
    display: "flex",
    alignItems: "center",
    padding: "0 12px",
    gap: 4,
  },
  separator: {
    width: 1,
    height: 24,
    background: "#444",
    margin: "0 8px",
  },
  button: {
    background: "#2a2a45",
    color: "#ccc",
    border: "1px solid #444",
    borderRadius: 4,
    padding: "4px 12px",
    fontSize: 13,
    cursor: "pointer",
    whiteSpace: "nowrap" as const,
    lineHeight: "22px",
  },
};

const Toolbar: React.FC<ToolbarProps> = ({ onAction }) => {
  const primitives = buttons.filter((b) => b.group === "primitives");
  const operations = buttons.filter((b) => b.group === "operations");

  const renderButton = (btn: ButtonDef) => (
    <button
      key={btn.action}
      style={styles.button}
      onClick={() => onAction(btn.action)}
      onMouseEnter={(e) => {
        (e.currentTarget as HTMLButtonElement).style.background = "#3a3a5e";
      }}
      onMouseLeave={(e) => {
        (e.currentTarget as HTMLButtonElement).style.background = "#2a2a45";
      }}
    >
      {btn.label}
    </button>
  );

  return (
    <div style={styles.toolbar}>
      {primitives.map(renderButton)}
      <div style={styles.separator} />
      {operations.map(renderButton)}
    </div>
  );
};

export default Toolbar;
