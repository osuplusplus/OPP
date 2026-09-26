import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { desktopApi } from "../../shared/lib/tauri";

export const localDatabaseKey = ["local-database"] as const;

export function useLocalDatabaseStatus() {
  return useQuery({ queryKey: localDatabaseKey, queryFn: desktopApi.getLocalDatabaseStatus, staleTime: Infinity, retry: false });
}

type DatabaseAction =
  | { kind: "recommended"; directory: string }
  | { kind: "choose" | "locate"; directory: string }
  | { kind: "retry" };

export function useLocalDatabaseAction() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async (action: DatabaseAction) => {
      if (action.kind === "retry") return desktopApi.retryLocalDatabase();
      if (action.kind === "recommended") return desktopApi.initializeLocalDatabase(action.directory);
      const selected = await desktopApi.chooseLocalDirectory(action.directory, "选择 OPP 本地数据库保存目录");
      if (!selected) return null;
      return action.kind === "locate"
        ? desktopApi.locateLocalDatabase(selected)
        : desktopApi.initializeLocalDatabase(selected);
    },
    onSuccess: (status) => {
      if (!status) return;
      queryClient.setQueryData(localDatabaseKey, status);
      void queryClient.invalidateQueries({ queryKey: ["local-library-storage"] });
      void queryClient.invalidateQueries({ queryKey: ["local-index-status"] });
      void queryClient.invalidateQueries({ queryKey: ["local-beatmap-presence"] });
    },
    onError: () => { void queryClient.invalidateQueries({ queryKey: localDatabaseKey }); },
  });
}

export function useLocalLibraryStorage(enabled: boolean) {
  return useQuery({ queryKey: ["local-library-storage"], queryFn: desktopApi.getLocalLibraryStorageStatus, enabled, refetchInterval: 5000, retry: false });
}

export function useMigrateLocalLibrary() {
  const client = useQueryClient();
  return useMutation({ mutationFn: desktopApi.migrateLocalLibraryDatabase,
    onSuccess: (status) => {
      client.setQueryData(["local-library-storage"], status);
      void client.invalidateQueries({ queryKey: ["local-beatmap-presence"] });
      void client.invalidateQueries({ queryKey: ["local-index-status"] });
    },
  });
}
