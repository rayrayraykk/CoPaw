# -*- coding: utf-8 -*-
import logging
from typing import Any
from pathlib import Path
from fastapi import APIRouter, HTTPException, Request
from pydantic import BaseModel, Field
from ...agents.skills_manager import (
    SkillService,
    SkillInfo,
)
from ...agents.skills_hub import (
    search_hub_skills,
    install_skill_from_hub,
)


logger = logging.getLogger(__name__)


class SkillSpec(SkillInfo):
    enabled: bool = False


class CreateSkillRequest(BaseModel):
    name: str = Field(..., description="Skill name")
    content: str = Field(..., description="Skill content (SKILL.md)")
    references: dict[str, Any] | None = Field(
        None,
        description="Optional tree structure for references/. "
        "Can be flat {filename: content} or nested "
        "{dirname: {filename: content}}",
    )
    scripts: dict[str, Any] | None = Field(
        None,
        description="Optional tree structure for scripts/. "
        "Can be flat {filename: content} or nested "
        "{dirname: {filename: content}}",
    )


class HubSkillSpec(BaseModel):
    slug: str
    name: str
    description: str = ""
    version: str = ""
    source_url: str = ""


class HubInstallRequest(BaseModel):
    bundle_url: str = Field(..., description="Skill URL")
    version: str = Field(default="", description="Optional version tag")
    enable: bool = Field(default=True, description="Enable after import")
    overwrite: bool = Field(
        default=False,
        description="Overwrite existing customized skill",
    )


router = APIRouter(prefix="/skills", tags=["skills"])


@router.get("")
async def list_skills(
    request: Request,
) -> list[SkillSpec]:
    """List all skills for active agent."""
    from ..agent_context import get_agent_for_request

    workspace = await get_agent_for_request(request)

    # List skills from agent's workspace
    workspace_dir = Path(workspace.workspace_dir)
    active_skills_dir = workspace_dir / "active_skills"
    customized_skills_dir = workspace_dir / "customized_skills"

    # Get builtin skills (global)
    builtin_skills_dir = (
        Path(__file__).parent.parent.parent / "agents" / "skills"
    )

    skills_spec = []

    # Collect from builtin
    if builtin_skills_dir.exists():
        for skill_dir in builtin_skills_dir.iterdir():
            if skill_dir.is_dir() and (skill_dir / "SKILL.md").exists():
                skill_md = skill_dir / "SKILL.md"
                is_enabled = (active_skills_dir / skill_dir.name).exists()
                skills_spec.append(
                    SkillSpec(
                        name=skill_dir.name,
                        content=skill_md.read_text(encoding="utf-8"),
                        source="builtin",
                        path=str(skill_dir),
                        references={},
                        scripts={},
                        enabled=is_enabled,
                    ),
                )

    # Collect from customized (override builtin)
    if customized_skills_dir.exists():
        for skill_dir in customized_skills_dir.iterdir():
            if skill_dir.is_dir() and (skill_dir / "SKILL.md").exists():
                skill_md = skill_dir / "SKILL.md"
                is_enabled = (active_skills_dir / skill_dir.name).exists()
                # Remove builtin with same name
                skills_spec = [
                    s for s in skills_spec if s.name != skill_dir.name
                ]
                skills_spec.append(
                    SkillSpec(
                        name=skill_dir.name,
                        content=skill_md.read_text(encoding="utf-8"),
                        source="customized",
                        path=str(skill_dir),
                        references={},
                        scripts={},
                        enabled=is_enabled,
                    ),
                )

    return skills_spec


@router.get("/available")
async def get_available_skills(
    request: Request,
) -> list[SkillSpec]:
    """List available (enabled) skills for active agent."""
    from ..agent_context import get_agent_for_request

    workspace = await get_agent_for_request(request)

    workspace_dir = Path(workspace.workspace_dir)
    active_skills_dir = workspace_dir / "active_skills"

    skills_spec = []
    if active_skills_dir.exists():
        for skill_dir in active_skills_dir.iterdir():
            if skill_dir.is_dir() and (skill_dir / "SKILL.md").exists():
                skill_md = skill_dir / "SKILL.md"
                skills_spec.append(
                    SkillSpec(
                        name=skill_dir.name,
                        content=skill_md.read_text(encoding="utf-8"),
                        source="active",
                        path=str(skill_dir),
                        references={},
                        scripts={},
                        enabled=True,
                    ),
                )

    return skills_spec


@router.get("/hub/search")
async def search_hub(
    q: str = "",
    limit: int = 20,
) -> list[HubSkillSpec]:
    results = search_hub_skills(q, limit=limit)
    return [
        HubSkillSpec(
            slug=item.slug,
            name=item.name,
            description=item.description,
            version=item.version,
            source_url=item.source_url,
        )
        for item in results
    ]


def _github_token_hint(bundle_url: str) -> str:
    """Hint to set GITHUB_TOKEN when URL is from GitHub/skills.sh."""
    if not bundle_url:
        return ""
    lower = bundle_url.lower()
    if "skills.sh" in lower or "github.com" in lower:
        return " Tip: set GITHUB_TOKEN (or GH_TOKEN) to avoid rate limits."
    return ""


@router.post("/hub/install")
async def install_from_hub(request: HubInstallRequest):
    try:
        result = install_skill_from_hub(
            bundle_url=request.bundle_url,
            version=request.version,
            enable=request.enable,
            overwrite=request.overwrite,
        )
    except ValueError as e:
        detail = str(e)
        logger.warning(
            "Skill hub install 400: bundle_url=%s detail=%s",
            (request.bundle_url or "")[:80],
            detail,
        )
        raise HTTPException(status_code=400, detail=detail) from e
    except RuntimeError as e:
        # Upstream hub is flaky/rate-limited sometimes; surface as bad gateway.
        detail = str(e) + _github_token_hint(request.bundle_url)
        logger.exception(
            "Skill hub install failed (upstream/rate limit): %s",
            e,
        )
        raise HTTPException(status_code=502, detail=detail) from e
    except Exception as e:
        detail = f"Skill hub import failed: {e}" + _github_token_hint(
            request.bundle_url,
        )
        logger.exception("Skill hub import failed: %s", e)
        raise HTTPException(status_code=502, detail=detail) from e
    return {
        "installed": True,
        "name": result.name,
        "enabled": result.enabled,
        "source_url": result.source_url,
    }


@router.post("/batch-disable")
async def batch_disable_skills(skill_name: list[str]) -> None:
    for skill in skill_name:
        SkillService.disable_skill(skill)


@router.post("/batch-enable")
async def batch_enable_skills(skill_name: list[str]) -> None:
    for skill in skill_name:
        SkillService.enable_skill(skill)


@router.post("")
async def create_skill(request: CreateSkillRequest):
    result = SkillService.create_skill(
        name=request.name,
        content=request.content,
        references=request.references,
        scripts=request.scripts,
    )
    return {"created": result}


@router.post("/{skill_name}/disable")
async def disable_skill(
    skill_name: str,
    request: Request = None,
):
    """Disable skill for active agent."""
    from ..agent_context import get_agent_for_request
    import shutil

    workspace = await get_agent_for_request(request)
    workspace_dir = Path(workspace.workspace_dir)
    active_skill_dir = workspace_dir / "active_skills" / skill_name

    if active_skill_dir.exists():
        shutil.rmtree(active_skill_dir)

        # Hot reload config (async, non-blocking)
        import asyncio

    async def reload_in_background():
        try:
            manager = request.app.state.multi_agent_manager
            await manager.reload_agent(workspace.agent_id)
        except Exception as e:
            logger.warning(f"Background reload failed: {e}")

        asyncio.create_task(reload_in_background())

        return {"disabled": True}

    return {"disabled": False}


@router.post("/{skill_name}/enable")
async def enable_skill(
    skill_name: str,
    request: Request = None,
):
    """Enable skill for active agent."""
    from ..agent_context import get_agent_for_request
    import shutil

    workspace = await get_agent_for_request(request)
    workspace_dir = Path(workspace.workspace_dir)
    active_skill_dir = workspace_dir / "active_skills" / skill_name

    # If already enabled, skip
    if active_skill_dir.exists():
        return {"enabled": True}

    # Find skill from builtin or customized
    builtin_skill_dir = (
        Path(__file__).parent.parent.parent / "agents" / "skills" / skill_name
    )
    customized_skill_dir = workspace_dir / "customized_skills" / skill_name

    source_dir = None
    if customized_skill_dir.exists():
        source_dir = customized_skill_dir
    elif builtin_skill_dir.exists():
        source_dir = builtin_skill_dir

    if not source_dir or not (source_dir / "SKILL.md").exists():
        raise HTTPException(
            status_code=404,
            detail=f"Skill '{skill_name}' not found",
        )

    # Copy to active_skills
    shutil.copytree(source_dir, active_skill_dir)

    # Hot reload config (async, non-blocking)
    import asyncio

    async def reload_in_background():
        try:
            manager = request.app.state.multi_agent_manager
            await manager.reload_agent(workspace.agent_id)
        except Exception as e:
            logger.warning(f"Background reload failed: {e}")

    asyncio.create_task(reload_in_background())

    return {"enabled": True}


@router.delete("/{skill_name}")
async def delete_skill(skill_name: str):
    """Delete a skill from customized_skills directory permanently.

    This only deletes skills from customized_skills directory.
    Built-in skills cannot be deleted.
    """
    result = SkillService.delete_skill(skill_name)
    return {"deleted": result}


@router.get("/{skill_name}/files/{source}/{file_path:path}")
async def load_skill_file(
    skill_name: str,
    source: str,
    file_path: str,
):
    """Load a specific file from a skill's references or scripts directory.

    Args:
        skill_name: Name of the skill
        source: Source directory ("builtin" or "customized")
        file_path: Path relative to skill directory, must start with
                   "references/" or "scripts/"

    Returns:
        File content as string, or None if not found

    Example:
        GET /skills/my_skill/files/customized/references/doc.md
        GET /skills/builtin_skill/files/builtin/scripts/utils/helper.py
    """
    content = SkillService.load_skill_file(
        skill_name=skill_name,
        file_path=file_path,
        source=source,
    )
    return {"content": content}
