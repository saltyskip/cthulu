import { createContext, useContext, useState, useEffect, useCallback, type ReactNode } from "react";
import * as api from "../api/client";
import type { Org } from "../types/flow";

interface OrgContextValue {
  orgs: Org[];
  selectedOrg: Org | null;
  selectedOrgSlug: string | null;
  loading: boolean;
  setSelectedOrgSlug: (slug: string) => void;
  reloadOrgs: () => Promise<void>;
  createOrg: (name: string, description?: string) => Promise<Org>;
  deleteOrg: (slug: string) => Promise<void>;
}

const OrgContext = createContext<OrgContextValue | null>(null);

export function OrgProvider({ children }: { children: ReactNode }) {
  const [orgs, setOrgs] = useState<Org[]>([]);
  const [selectedOrgSlug, setSelectedOrgSlugState] = useState<string | null>(() => {
    return localStorage.getItem("cthulu.selectedOrg");
  });
  const [loading, setLoading] = useState(true);

  const reloadOrgs = useCallback(async () => {
    try {
      const list = await api.listOrgs();
      setOrgs(list);
      // Auto-select first org if none selected or current selection invalid
      setSelectedOrgSlugState(prev => {
        if (prev && list.some(o => o.slug === prev)) return prev;
        return list.length > 0 ? list[0].slug : null;
      });
    } catch {
      /* ignore */
    }
    setLoading(false);
  }, []);

  useEffect(() => {
    reloadOrgs();
  }, [reloadOrgs]);

  const setSelectedOrgSlug = useCallback((slug: string) => {
    setSelectedOrgSlugState(slug);
    localStorage.setItem("cthulu.selectedOrg", slug);
  }, []);

  const selectedOrg = orgs.find(o => o.slug === selectedOrgSlug) ?? null;

  const handleCreateOrg = useCallback(async (name: string, description?: string): Promise<Org> => {
    const result = await api.createOrg(name, description);
    await reloadOrgs();
    setSelectedOrgSlug(result.slug);
    return { slug: result.slug, name: result.name, description: description ?? "" };
  }, [reloadOrgs, setSelectedOrgSlug]);

  const handleDeleteOrg = useCallback(async (slug: string) => {
    await api.deleteOrg(slug);
    const remaining = orgs.filter(o => o.slug !== slug);
    setOrgs(remaining);
    if (selectedOrgSlug === slug) {
      const next = remaining.length > 0 ? remaining[0].slug : null;
      if (next) setSelectedOrgSlug(next);
      else setSelectedOrgSlugState(null);
    }
  }, [orgs, selectedOrgSlug, setSelectedOrgSlug]);

  return (
    <OrgContext.Provider value={{
      orgs, selectedOrg, selectedOrgSlug, loading,
      setSelectedOrgSlug, reloadOrgs,
      createOrg: handleCreateOrg, deleteOrg: handleDeleteOrg,
    }}>
      {children}
    </OrgContext.Provider>
  );
}

export function useOrg() {
  const ctx = useContext(OrgContext);
  if (!ctx) throw new Error("useOrg must be used within OrgProvider");
  return ctx;
}
