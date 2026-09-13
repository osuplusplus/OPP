import { lazy, Suspense } from "react";
import { Navigate, Route, Routes, useLocation, useSearchParams } from "react-router-dom";
import { AppShell } from "./AppShell";
import { AppLoading } from "./AppLoading";

// Keep feature bundles at the route boundary: the shell becomes interactive
// without eagerly loading charts, local scans, and tooling screens.
const DataCenterPage = lazy(() => import("../features/profile/DataCenterPage").then((module) => ({ default: module.DataCenterPage })));
const OverviewPage = lazy(() => import("../features/profile/OverviewPage").then((module) => ({ default: module.OverviewPage })));
const ProfileDetailsPage = lazy(() => import("../features/profile/ProfileDetailsPage").then((module) => ({ default: module.ProfileDetailsPage })));
const MedalsPage = lazy(() => import("../features/profile/MedalsPage").then((module) => ({ default: module.MedalsPage })));
const CareerPage = lazy(() => import("../features/profile/CareerPage").then((module) => ({ default: module.CareerPage })));
const ScoresPage = lazy(() => import("../features/scores/ScoresPage").then((module) => ({ default: module.ScoresPage })));
const OnlineBeatmapsPage = lazy(() => import("../features/online-beatmaps/OnlineBeatmapsPage").then((module) => ({ default: module.OnlineBeatmapsPage })));
const SimilarBeatmapsPage = lazy(() => import("../features/similar-beatmaps/SimilarBeatmapsPage").then((module) => ({ default: module.SimilarBeatmapsPage })));
const LocalAnalysisPage = lazy(() => import("../features/local-analysis/LocalAnalysisPage").then((module) => ({ default: module.LocalAnalysisPage })));
const SkinWorkshopPage = lazy(() => import("../features/skin-workshop/SkinWorkshopPage").then((module) => ({ default: module.SkinWorkshopPage })));
const LocalMediaPage = lazy(() => import("../features/local-media/LocalMediaPage").then((module) => ({ default: module.LocalMediaPage })));
const ReplayRenderPage = lazy(() => import("../features/local-media/ReplayRenderPage").then((module) => ({ default: module.ReplayRenderPage })));
const SettingsPage = lazy(() => import("../features/settings/SettingsPage").then((module) => ({ default: module.SettingsPage })));
const ToolsLayout = lazy(() => import("../features/tools/ToolsLayout").then((module) => ({ default: module.ToolsLayout })));
const ToolCategoryPages = {
  game: lazy(() => import("../features/tools/ToolCategoryPages").then((module) => ({ default: module.GameToolsPage }))),
  beatmaps: lazy(() => import("../features/tools/ToolCategoryPages").then((module) => ({ default: module.BeatmapToolsPage }))),
  system: lazy(() => import("../features/tools/ToolCategoryPages").then((module) => ({ default: module.SystemToolsPage }))),
  live: lazy(() => import("../features/tools/ToolCategoryPages").then((module) => ({ default: module.LiveToolsPage }))),
};
const ViewTrainerPage = lazy(() => import("../features/view-trainer/ViewTrainerPage").then((module) => ({ default: module.ViewTrainerPage })));
const CollectionsPage = lazy(() => import("../features/collections/CollectionsPage").then((module) => ({ default: module.CollectionsPage })));
const BeatmapHubPage = lazy(() => import("../features/beatmaphub/BeatmapHubPage").then((module) => ({ default: module.BeatmapHubPage })));

function LegacyTrainerRedirect() {
  const location = useLocation();
  return <Navigate replace to={`/view-trainer${location.search}`} />;
}

function ToolsIndexRedirect() {
  const [params] = useSearchParams();
  const query = params.get("preview_bid");
  return <Navigate replace to={query ? `/tools/beatmaps?preview_bid=${encodeURIComponent(query)}` : "/tools/game"} />;
}

function TosuRedirect() {
  const location = useLocation();
  return <Navigate replace to={`/tools/live${location.search}`} />;
}

export function AppRoutes() {
  return (
    <Suspense fallback={<AppLoading />}>
      <Routes>
        <Route element={<AppShell />}>
          <Route index element={<Navigate replace to="/online/beatmaps" />} />
          <Route path="/data" element={<DataCenterPage />}>
            <Route index element={<Navigate replace to="overview" />} />
            <Route path="overview" element={<OverviewPage />} />
            <Route path="scores" element={<ScoresPage />} />
            <Route path="recent" element={<ScoresPage category="recent" title="近期成绩" />} />
            <Route path="pinned" element={<ScoresPage category="pinned" title="Pinned 成绩" />} />
            <Route path="medals" element={<MedalsPage />} />
            <Route path="career" element={<CareerPage />} />
            <Route path="profile" element={<ProfileDetailsPage />} />
          </Route>
          <Route path="/online/overview" element={<Navigate replace to="/data/overview" />} />
          <Route path="/online/profile" element={<Navigate replace to="/data/profile" />} />
          <Route path="/online/scores" element={<Navigate replace to="/data/scores" />} />
          <Route path="/online/beatmaps" element={<OnlineBeatmapsPage />} />
          <Route path="/collections" element={<CollectionsPage />} />
          <Route path="/beatmaphub" element={<BeatmapHubPage />} />
          <Route path="/online/similar" element={<SimilarBeatmapsPage />} />
          <Route path="/trainer" element={<LegacyTrainerRedirect />} />
          <Route path="/view-trainer" element={<ViewTrainerPage />} />
          <Route path="/local" element={<Navigate replace to="/local/maps" />} />
          <Route path="/local/maps" element={<LocalAnalysisPage section="maps" />} />
          <Route path="/local/skins" element={<SkinWorkshopPage />} />
          <Route path="/local/media" element={<LocalMediaPage />} />
          <Route path="/local/media/screenshots" element={<Navigate replace to="/local/media?type=screenshot" />} />
          <Route path="/local/media/replays" element={<Navigate replace to="/local/media?type=replay" />} />
          <Route path="/local/media/render" element={<ReplayRenderPage />} />
          <Route path="/settings" element={<SettingsPage />} />
          <Route path="/game" element={<Navigate replace to="/online/beatmaps" />} />
          <Route path="/tools" element={<ToolsLayout />}>
            <Route index element={<ToolsIndexRedirect />} />
            <Route path="game" element={<ToolCategoryPages.game />} />
            <Route path="beatmaps" element={<ToolCategoryPages.beatmaps />} />
            <Route path="system" element={<ToolCategoryPages.system />} />
            <Route path="live" element={<ToolCategoryPages.live />} />
          </Route>
          <Route path="/tools/replay-render" element={<Navigate replace to="/local/media/render" />} />
          <Route path="/tosu" element={<TosuRedirect />} />
          <Route path="*" element={<Navigate replace to="/online/beatmaps" />} />
        </Route>
      </Routes>
    </Suspense>
  );
}
