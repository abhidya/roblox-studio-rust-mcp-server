--!strict
-- Shared prop-hunt configuration and types.
-- This is game LOGIC config. The world content (props, set pieces) is filled
-- by the asset-driven-game-design skill, not hard-coded here.

export type Phase = "Intermission" | "Hiding" | "Hunting" | "RoundEnd"

export type RoundState = {
	phase: Phase,
	endsAt: number, -- os.clock() deadline for the current phase
	seekers: { number }, -- UserIds
	hiders: { number }, -- UserIds
	taggedHiders: { number },
}

local Config = {
	-- Folder in Workspace that the asset-driven build populates with valid,
	-- inspected, single-model disguise props.
	HIDEABLE_PROPS_FOLDER = "HideableProps",

	-- Phase durations (seconds).
	INTERMISSION_TIME = 15,
	HIDING_TIME = 30,
	HUNTING_TIME = 120,
	ROUND_END_TIME = 8,

	-- Player counts.
	MIN_PLAYERS = 2,
	SEEKER_RATIO = 0.25, -- fraction of players that become seekers (min 1)

	-- How close a hider must be to a prop to disguise as it (studs).
	DISGUISE_RANGE = 16,
	-- How close a seeker must be to tag a disguised hider (studs).
	TAG_RANGE = 8,

	-- Scoring.
	POINTS_PER_SURVIVED_HIDER = 10,
	POINTS_PER_TAG = 25,
}

return Config
