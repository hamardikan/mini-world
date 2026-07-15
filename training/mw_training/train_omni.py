"""CPU-friendly OMNI behavior-cloning trainer and dynamic-manifest ONNX export."""

from __future__ import annotations

import argparse
from dataclasses import asdict, dataclass
import json
from pathlib import Path
from typing import Sequence

import torch
from torch import nn
from torch.utils.data import DataLoader

from .dataset import IGNORE_INDEX, N_NEIGHBORS
from .omni import DEFAULT_DESCRIPTOR_DIM, DEFAULT_PARAM_DIM, OmniDataset, OmniPolicy, count_parameters

OPSET_VERSION = 17


@dataclass
class OmniTrainConfig:
    seed: int = 20260715
    epochs: int = 30
    batch_size: int = 32
    learning_rate: float = 2e-3
    weight_decay: float = 1e-4
    patience: int = 8
    hidden_dim: int = 96
    target_weight: float = 0.1
    param_weight: float = 0.05


def _seed(seed: int) -> None:
    torch.manual_seed(seed)


def _step(model: OmniPolicy, batch, device: torch.device) -> tuple[torch.Tensor, torch.Tensor, torch.Tensor]:
    obs, descriptors, afforded, tool, target, params = (x.to(device) for x in batch)
    scores, target_logits, param_values = model(obs, descriptors, afforded)
    tool_loss = nn.functional.cross_entropy(scores, tool)
    target_rows = target != IGNORE_INDEX
    target_loss = (
        nn.functional.cross_entropy(target_logits[target_rows], target[target_rows])
        if target_rows.any()
        else tool_loss.new_zeros(())
    )
    param_loss = nn.functional.mse_loss(param_values, params)
    return tool_loss + model.target_weight * target_loss + model.param_weight * param_loss, scores.argmax(-1), tool


@torch.no_grad()
def _evaluate(model: OmniPolicy, data: OmniDataset, device: torch.device, batch_size: int) -> dict[str, float]:
    model.eval()
    total_loss = total = correct = 0.0
    for batch in DataLoader(data, batch_size=batch_size, shuffle=False):
        loss, prediction, truth = _step(model, batch, device)
        count = truth.numel()
        total_loss += float(loss) * count
        total += count
        correct += float((prediction == truth).sum())
    return {"loss": total_loss / max(total, 1), "match_rate": correct / max(total, 1), "count": total}


def train_omni(
    train_data: OmniDataset,
    heldout_data: OmniDataset | None = None,
    config: OmniTrainConfig | None = None,
    *,
    device: torch.device | None = None,
) -> tuple[OmniPolicy, dict]:
    """Fit OMNI and return the best held-out tool-match checkpoint plus history."""
    config = config or OmniTrainConfig()
    _seed(config.seed)
    device = device or torch.device("cpu")
    model = OmniPolicy(
        obs_dim=train_data.obs.shape[-1],
        descriptor_dim=train_data.tool_descriptors.shape[-1],
        hidden_dim=config.hidden_dim,
        target_slots=N_NEIGHBORS,
        param_dim=train_data.params.shape[-1],
    ).to(device)
    # Keep loss weights on the module for the small _step helper; they are not exported.
    model.target_weight = config.target_weight
    model.param_weight = config.param_weight
    heldout_data = heldout_data or train_data
    loader = DataLoader(
        train_data,
        batch_size=config.batch_size,
        shuffle=True,
        generator=torch.Generator().manual_seed(config.seed),
    )
    optimizer = torch.optim.AdamW(
        model.parameters(),
        lr=config.learning_rate,
        weight_decay=config.weight_decay,
    )
    best_state = None
    best_score = float("-inf")
    stale = 0
    history: list[dict[str, float]] = []
    for epoch in range(1, config.epochs + 1):
        model.train()
        for batch in loader:
            optimizer.zero_grad(set_to_none=True)
            loss, _, _ = _step(model, batch, device)
            loss.backward()
            optimizer.step()
        train_metrics = _evaluate(model, train_data, device, config.batch_size)
        heldout_metrics = _evaluate(model, heldout_data, device, config.batch_size)
        row = {
            "epoch": epoch,
            "train_loss": train_metrics["loss"],
            "heldout_loss": heldout_metrics["loss"],
            "train_match": train_metrics["match_rate"],
            "heldout_match": heldout_metrics["match_rate"],
        }
        history.append(row)
        print(
            f"epoch={epoch:02d} train_loss={row['train_loss']:.4f} "
            f"heldout_loss={row['heldout_loss']:.4f} "
            f"train_match={row['train_match']:.3f} "
            f"heldout_match={row['heldout_match']:.3f}",
            flush=True,
        )
        if row["heldout_match"] > best_score:
            best_score = row["heldout_match"]
            best_state = {key: value.detach().cpu().clone() for key, value in model.state_dict().items()}
            stale = 0
        else:
            stale += 1
            if stale >= config.patience:
                break
    if best_state is None:
        raise RuntimeError("OMNI training produced no checkpoint")
    model.load_state_dict(best_state)
    final_train = _evaluate(model, train_data, device, config.batch_size)
    final_heldout = _evaluate(model, heldout_data, device, config.batch_size)
    metrics = {
        "params": count_parameters(model),
        "device": str(device),
        "train_match_rate": final_train["match_rate"],
        "heldout_match_rate": final_heldout["match_rate"],
        "train_loss": final_train["loss"],
        "heldout_loss": final_heldout["loss"],
        "history": history,
        "config": asdict(config),
        "descriptor_dim": train_data.tool_descriptors.shape[-1],
        "tool_count": train_data.tool_descriptors.shape[1],
        "param_dim": train_data.params.shape[-1],
        "opset": OPSET_VERSION,
    }
    return model.cpu(), metrics


def export_omni_onnx(model: OmniPolicy, path: str | Path) -> None:
    """Export dynamic batch/tool-count OMNI graph with the stable I/O contract."""
    model = model.eval()
    tools = 3
    obs = torch.zeros((1, model.obs_dim), dtype=torch.float32)
    descriptors = torch.zeros((1, tools, model.descriptor_dim), dtype=torch.float32)
    afforded = torch.ones((1, tools), dtype=torch.float32)
    torch.onnx.export(
        model,
        (obs, descriptors, afforded),
        str(path),
        input_names=["obs", "tool_descriptors", "afforded"],
        output_names=["tool_scores", "target_logits", "params"],
        dynamic_axes={
            "obs": {0: "batch"},
            "tool_descriptors": {0: "batch", 1: "tools"},
            "afforded": {0: "batch", 1: "tools"},
            "tool_scores": {0: "batch", 1: "tools"},
            "target_logits": {0: "batch"},
            "params": {0: "batch"},
        },
        opset_version=OPSET_VERSION,
        dynamo=False,
    )


def main(argv: Sequence[str] | None = None) -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--fixtures", default="training/artifacts/fixtures.json")
    parser.add_argument("--out", default="training/artifacts/omni.onnx")
    parser.add_argument("--epochs", type=int, default=30)
    parser.add_argument("--hidden-dim", type=int, default=96)
    args = parser.parse_args(argv)
    data = OmniDataset.from_fixtures(args.fixtures)
    model, metrics = train_omni(data, config=OmniTrainConfig(epochs=args.epochs, hidden_dim=args.hidden_dim))
    export_omni_onnx(model, args.out)
    Path(args.out).with_suffix(".metrics.json").write_text(json.dumps(metrics, indent=2) + "\n", encoding="utf-8")
    print(f"params={metrics['params']} heldout_match={metrics['heldout_match_rate']:.3%} artifact={args.out}")


if __name__ == "__main__":
    main()
