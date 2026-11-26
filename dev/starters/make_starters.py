"""Makes the starter workflows for ProCraft Diffusion: each in ComfyUI's regular (UI) format, to
open in ComfyUI so its tab sends it to the app, and in the API format, as the app reads it (a test
fixture). One description per workflow; both files come from it."""

import json
import sys

# A node: id, class, widget values (in the node's widget order), inputs fed by links
# ({input name: (from node, from slot)}), outputs (name, type), and which inputs are widgets
# given a link (their widget names, so the regular format marks them).
MODEL = ("z_image_turbo_bf16.safetensors", "qwen_3_4b.safetensors", "ae.safetensors")


def starter(paint_over: bool):
    unet, te, vae = MODEL
    nodes = []

    def node(nid, cls, widgets, links, outputs, widget_links=(), api_widgets=None, title=None, pos=(0, 0)):
        nodes.append(dict(id=nid, cls=cls, widgets=widgets, links=links, outputs=outputs,
                          widget_links=set(widget_links), api=api_widgets or {}, title=title, pos=pos))

    node(1, "UNETLoader", [unet, "default"], {}, [("MODEL", "MODEL")],
         api_widgets={"unet_name": unet, "weight_dtype": "default"}, pos=(0, 0))
    node(2, "ModelSamplingAuraFlow", [3], {"model": (1, 0)}, [("MODEL", "MODEL")],
         api_widgets={"shift": 3}, pos=(340, 0))
    node(3, "CLIPLoader", [te, "lumina2", "default"], {}, [("CLIP", "CLIP")],
         api_widgets={"clip_name": te, "type": "lumina2", "device": "default"}, pos=(0, 140))
    node(4, "VAELoader", [vae], {}, [("VAE", "VAE")], api_widgets={"vae_name": vae}, pos=(0, 300))
    node(5, "ETN_Parameter", ["Prompt", "prompt (positive)", "", -1e10, 1e10], {}, [("value", "*")],
         api_widgets={"name": "Prompt", "type": "prompt (positive)", "default": "", "min": -1e10, "max": 1e10},
         title="Prompt (from the app)", pos=(0, 420))
    node(6, "CLIPTextEncode", [""], {"clip": (3, 0), "text": (5, 0)}, [("CONDITIONING", "CONDITIONING")],
         widget_links=["text"], pos=(340, 140))
    node(7, "ConditioningZeroOut", [], {"conditioning": (6, 0)}, [("CONDITIONING", "CONDITIONING")], pos=(700, 140))
    node(8, "ETN_KritaCanvas", [], {}, [("image", "IMAGE"), ("width", "INT"), ("height", "INT"), ("seed", "INT"), ("mask", "MASK")],
         title="Canvas (from the app)", pos=(0, 600))
    node(9, "ImageScaleToTotalPixels", ["lanczos", 1.0, 16], {"image": (8, 0)}, [("IMAGE", "IMAGE")],
         api_widgets={"upscale_method": "lanczos", "megapixels": 1.0, "resolution_steps": 16}, pos=(340, 600))
    node(10, "VAEEncode", [], {"pixels": (9, 0), "vae": (4, 0)}, [("LATENT", "LATENT")], pos=(700, 600))
    latent = (10, 0)
    denoise = 1.0
    if paint_over:
        node(11, "ETN_KritaSelection", ["automatic", 32], {}, [("mask", "MASK"), ("active", "BOOLEAN"), ("offset x", "INT"), ("offset y", "INT")],
             api_widgets={"context": "automatic", "padding": 32}, title="Selection (from the app)", pos=(340, 760))
        node(12, "SetLatentNoiseMask", [], {"samples": (10, 0), "mask": (11, 0)}, [("LATENT", "LATENT")], pos=(1000, 600))
        latent = (12, 0)
        node(13, "ETN_Parameter", ["Strength", "number", "0.6", 0.05, 1.0], {}, [("value", "*")],
             api_widgets={"name": "Strength", "type": "number", "default": "0.6", "min": 0.05, "max": 1.0},
             title="Strength (from the app)", pos=(700, 400))
    sampler_links = {"model": (2, 0), "positive": (6, 0), "negative": (7, 0), "latent_image": latent, "seed": (8, 3)}
    sampler_widget_links = ["seed"]
    api = {"steps": 8, "cfg": 1, "sampler_name": "res_multistep", "scheduler": "simple"}
    if paint_over:
        sampler_links["denoise"] = (13, 0)
        sampler_widget_links.append("denoise")
    else:
        api["denoise"] = denoise
    node(14, "KSampler", [0, "fixed", 8, 1, "res_multistep", "simple", denoise], sampler_links, [("LATENT", "LATENT")],
         widget_links=sampler_widget_links, api_widgets=api, pos=(1300, 140))
    node(15, "VAEDecode", [], {"samples": (14, 0), "vae": (4, 0)}, [("IMAGE", "IMAGE")], pos=(1650, 140))
    node(16, "ETN_KritaOutput", [0, 0, "", "default", False], {"images": (15, 0)}, [],
         api_widgets={"x": 0, "y": 0, "name": "", "batch_mode": "default", "resize_canvas": False},
         title="Back to the app", pos=(1900, 140))
    return nodes


def widget_inputs(n):
    # the CLIPTextEncode's text, KSampler's seed and denoise: widgets given a link
    types = {"text": "STRING", "seed": "INT", "denoise": "FLOAT"}
    return [(w, types.get(w, "*")) for w in n["widget_links"]]


def to_ui(nodes, name):
    links = []
    ui_nodes = []
    link_id = 0
    by_id = {n["id"]: n for n in nodes}
    out_links = {}
    for n in nodes:
        for inp, (src, slot) in n["links"].items():
            link_id += 1
            typ = by_id[src]["outputs"][slot][1]
            links.append([link_id, src, slot, n["id"], inp, typ])
            out_links.setdefault((src, slot), []).append(link_id)
    for order, n in enumerate(nodes):
        inputs = []
        slot = 0
        index = {}
        for inp, (src, s) in n["links"].items():
            lid = next(l[0] for l in links if l[3] == n["id"] and l[4] == inp)
            entry = {"name": inp, "type": by_id[src]["outputs"][s][1], "link": lid}
            if inp in n["widget_links"]:
                entry["widget"] = {"name": inp}
                entry["type"] = dict(widget_inputs(n)).get(inp, "*")
            inputs.append(entry)
            index[inp] = slot
            slot += 1
        outputs = [{"name": oname, "type": otype, "links": out_links.get((n["id"], i), []), "slot_index": i}
                   for i, (oname, otype) in enumerate(n["outputs"])]
        ui = {"id": n["id"], "type": n["cls"], "pos": list(n["pos"]), "size": [300, 100], "flags": {}, "order": order,
              "mode": 0, "inputs": inputs, "outputs": outputs, "properties": {"Node name for S&R": n["cls"]},
              "widgets_values": n["widgets"]}
        if n["title"]:
            ui["title"] = n["title"]
        ui_nodes.append(ui)
    # links refer to target slots by index
    for l in links:
        target = next(x for x in ui_nodes if x["id"] == l[3])
        l[4] = next(i for i, e in enumerate(target["inputs"]) if e["name"] == l[4])
    note = {"id": 99, "type": "MarkdownNote", "pos": [0, -260], "size": [620, 220], "flags": {}, "order": len(ui_nodes),
            "mode": 0, "inputs": [], "outputs": [], "properties": {},
            "widgets_values": [f"## {name}\n\nMade for ProCraft Diffusion. Keep this workflow **open in ComfyUI**: the app on your phone gets it as the **ComfyUI (live)** card. On the phone, hold that card and choose **Keep** to keep it for good.\n\nThe nodes marked *(from the app)* are filled in by the app: your canvas, your prompt, the selection. **Back to the app** sends the result to your phone. Pressing Run here does nothing useful."]}
    ui_nodes.append(note)
    return {"id": "procraft-" + name.lower().replace(" ", "-"), "revision": 0, "last_node_id": 99,
            "last_link_id": link_id, "nodes": ui_nodes, "links": links, "groups": [], "config": {},
            "extra": {"ds": {"scale": 0.7, "offset": [60, 320]}}, "version": 0.4}


def to_api(nodes):
    out = {}
    for n in nodes:
        inputs = dict(n["api"])
        if n["cls"] == "CLIPTextEncode":
            pass
        for inp, (src, slot) in n["links"].items():
            inputs[inp] = [str(src), slot]
        out[str(n["id"])] = {"class_type": n["cls"], "inputs": inputs}
    return out


if __name__ == "__main__":
    where = sys.argv[1]
    for paint_over, name in [(False, "ProCraft - Z-Image new picture"), (True, "ProCraft - Z-Image paint over")]:
        nodes = starter(paint_over)
        json.dump(to_ui(nodes, name), open(f"{where}/{name}.json", "w"), indent=2)
        json.dump(to_api(nodes), open(f"{where}/{name} (API).json", "w"), indent=2)
        print("made", name)
