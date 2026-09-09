# 🐕 Conduit 1.0 Milestone Roadmap & Feature Audit

This document outlines the architectural roadmap, planned features, and hardening tasks for the **Conduit 1.0 Release**.

---

## 🎯 1.0 Release Vision
Conduit provides a unified, multi-daemon control plane, real-time telemetry hub, and full-lifecycle automation daemon across Synapse/Transmission/qBittorrent/Deluge fetchers, Sonarr/Radarr/Lidarr media managers, Plex/Jellyfin streaming servers, and Ombi/Overseerr request systems.

---

## 🚀 1.0 Milestones & Features

### 📡 1. Multi-Daemon Fetcher Support
- [x] **qBittorrent Web API Adapter (`v2.x`)**:
  - Implement RPC translation layer for qBittorrent nodes (`/api/v2/torrents/*`).
  - Support compound IDs (`node:hash`), piece map extraction, bandwidth curves, and file priority toggles.
- [x] **Deluge JSON-RPC / Web Adapter**:
  - Support Deluge daemon connections alongside Synapse, Transmission, and qBittorrent.
- [x] **Cross-Node Torrent Migration**:
  - One-click migration of torrents across nodes/servers with fast-resume data generation and automated payload hardlink/copy.

### 🔄 2. Interactive Media Pipeline & Arr Control
- [x] **Interactive Search & Release Picker in Conduit UI**:
  - Surface Sonarr/Radarr interactive search releases directly within Conduit's Pipeline modal.
  - One-click manual grab override without switching to external Arr UIs.
- [x] **Bazarr Subtitles Webhook Ingestion & Integration**:
  - Ingest subtitle download/sync events from Bazarr.
  - Link subtitle acquisition into the Living Card timeline and Pipeline Browser.
- [x] **Jellyfin / Emby Media Server Support**:
  - Add native webhook ingestion and targeted library refresh for Jellyfin/Emby alongside Plex.
- [x] **Overseerr / Jellyseerr Ingestion**:
  - Direct webhook support for Overseerr/Jellyseerr request platforms with full living card integration.

### 🌐 3. Multi-Tenant Zones & Quota Management
- [ ] **Zone Disk Quotas & Bandwidth Throttling**:
  - Set hard/soft storage quotas and max download/upload rates per Zone.
- [ ] **Per-Zone Notification Rules & Schedules**:
  - Quiet hours per zone / target.
  - Batching/digest mode for high-volume intake periods.

### 📱 4. Conduit Mobile App (Flutter) 1.0 Production Readiness
- [ ] **Push Notifications (FCM / APNs / UnifiedPush)**:
  - Native mobile alerts for downloads staged, torrents broken/recovered, and kibble requests.
- [ ] **Interactive Bandwidth Charts & Contributor Breakdown**:
  - Touch-interactive live bandwidth history on mobile with pinch-to-zoom.
- [ ] **Biometric Unlock (FaceID / Fingerprint)**:
  - Secure biometric app lock for stored pairing credentials.

### 🔒 5. Enterprise Security, SSO & Authentication
- [ ] **OIDC / OAuth2 SSO Integration**:
  - Support SSO login via Authentik, Authelia, Keycloak, and Google OAuth.
- [ ] **Role-Based Access Control (RBAC)**:
  - Multi-user support with `Admin`, `Operator`, `Viewer`, and `Requester` permission tiers.
- [ ] **Automated S3 / WebDAV Off-site Encrypted Backup**:
  - Scheduled daily/weekly database and configuration backups pushed to S3/Backblaze B2/WebDAV.

### 🩺 6. Diagnostics, Observability & Hardening
- [ ] **Hardlink & Symlink Staging Health Checker**:
  - Background daemon to verify file integrity and detect broken links between download store and library.
- [ ] **Packaged Grafana Dashboards & Prometheus Alerts**:
  - Ready-to-import Grafana dashboard templates for cluster bandwidth, swarm health, and Arr pipeline throughput.
