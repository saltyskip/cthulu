import { useState } from "react";
import { useOrg } from "../contexts/OrgContext";
import { Plus } from "lucide-react";
import { NewOrgDialog } from "./NewOrgDialog";

export function OrgRail() {
  const { orgs, selectedOrgSlug, setSelectedOrgSlug } = useOrg();
  const [showNewOrg, setShowNewOrg] = useState(false);

  return (
    <div className="org-rail">
      <div className="org-rail-list">
        {orgs.map(org => {
          const initials = org.name.slice(0, 2).toUpperCase();
          const isSelected = org.slug === selectedOrgSlug;
          return (
            <button
              key={org.slug}
              className={`org-rail-item${isSelected ? " org-rail-item-active" : ""}`}
              onClick={() => setSelectedOrgSlug(org.slug)}
              title={org.name}
            >
              {isSelected && <div className="org-rail-indicator" />}
              <div className="org-rail-avatar">{initials}</div>
            </button>
          );
        })}
      </div>
      <button
        className="org-rail-add"
        onClick={() => setShowNewOrg(true)}
        title="Create organization"
      >
        <Plus size={16} />
      </button>
      {showNewOrg && <NewOrgDialog onClose={() => setShowNewOrg(false)} />}
    </div>
  );
}
