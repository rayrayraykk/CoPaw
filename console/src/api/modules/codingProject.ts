import { request } from "../request";
import { getApiUrl } from "../config";
import { buildAuthHeaders } from "../authHeaders";

export interface CodingProjectInfo {
  path: string;
  name: string;
  is_workspace_default: boolean;
  exists?: boolean;
}

export interface ProjectListItem {
  path: string;
  name: string;
  is_git: boolean;
  is_active: boolean;
}

export const codingProjectApi = {
  /** Get the current active coding project. */
  get: () => request<CodingProjectInfo>("/workspace/coding-project"),

  /**
   * Set the active coding project.
   * Pass `path: null` to reset to the default workspace.
   */
  set: (path: string | null) =>
    request<CodingProjectInfo>("/workspace/coding-project", {
      method: "PUT",
      body: JSON.stringify({ path }),
    }),

  /** Create a new empty project directory and git init it. */
  create: (name: string) =>
    request<{ path: string; name: string }>("/workspace/coding-project/create", {
      method: "POST",
      body: JSON.stringify({ name }),
    }),

  /** List all coding projects under the agent's coding_projects/ directory. */
  list: () => request<ProjectListItem[]>("/workspace/coding-project/list"),

  /**
   * Copy a local directory into coding_projects/ (excludes node_modules etc.)
   * and set it as the active project.
   */
  importLocal: (path: string, name?: string) =>
    request<{ path: string; name: string }>("/workspace/coding-project/import-local", {
      method: "POST",
      body: JSON.stringify({ path, name: name || undefined }),
    }),

  /**
   * Upload a zip of a project folder; backend extracts it to coding_projects/.
   * Same pattern as the plugin install modal.
   */
  uploadZip: (zipFile: File, name: string): Promise<{ path: string; name: string }> => {
    const formData = new FormData();
    formData.append("file", zipFile);
    return request<{ path: string; name: string }>(
      `/workspace/coding-project/upload-zip?name=${encodeURIComponent(name)}`,
      { method: "POST", body: formData },
    );
  },

  /** Low-level: POST to clone endpoint and return a ReadableStream of SSE. */
  cloneStream: (url: string, name?: string): Promise<Response> =>
    fetch(getApiUrl("/workspace/coding-project/clone"), {
      method: "POST",
      headers: {
        ...buildAuthHeaders(),
        "Content-Type": "application/json",
      },
      body: JSON.stringify({ url, name: name || undefined }),
    }),
};
